use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod buffer;
mod check;
mod collectors;
mod config;
mod connection;
mod credentials;
mod rpc;
mod snapshot;
mod tls;

use config::{AgentConfig, DEFAULT_CONFIG_PATH};

#[derive(Parser)]
#[command(name = "em-agent", version, about = "Agente de EM Monitor para hosts con Docker/Podman")]
struct Cli {
    /// Archivo de configuración
    #[arg(long, short, global = true, env = "EM_AGENT_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Recolecta y muestra un snapshot en JSON, sin conectarse al central
    Collect {
        /// Segundos entre la muestra base y la medida (para calcular CPU y tasas)
        #[arg(long, default_value_t = 2)]
        sample_secs: u64,
        /// JSON en una sola línea
        #[arg(long)]
        compact: bool,
    },
    /// Diagnostica sockets, permisos y configuración
    Check,
    /// Se conecta al central y envía snapshots periódicamente (modo servicio)
    Run,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Los logs van a stderr para que `collect` deje stdout limpio con el JSON
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "em_agent=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let explicit = cli.config.is_some();
    let path = cli.config.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
    let cfg = AgentConfig::load(&path, explicit)?;

    match cli.command {
        Command::Collect { sample_secs, compact } => collect_once(cfg, sample_secs, compact).await,
        Command::Run => run(cfg).await,
        Command::Check => {
            if !check::run(&cfg).await {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

async fn run(cfg: AgentConfig) -> Result<()> {
    let creds = credentials::Credentials::load(&cfg.credentials_file)?;
    let (clients, errors) = collectors::runtime::discover(&cfg.runtimes).await;
    for (socket, err) in &errors {
        tracing::warn!("{socket}: {err:#}");
    }
    let collector = snapshot::Collector::new(cfg.clone(), clients);
    connection::run(cfg, creds, collector).await
}

async fn collect_once(cfg: AgentConfig, sample_secs: u64, compact: bool) -> Result<()> {
    let (clients, errors) = collectors::runtime::discover(&cfg.runtimes).await;
    for (socket, err) in &errors {
        tracing::warn!("{socket}: {err:#}");
    }

    let mut collector = snapshot::Collector::new(cfg, clients);
    // Primera pasada: línea base para que la segunda tenga deltas
    collector.snapshot(true).await;
    tokio::time::sleep(Duration::from_secs(sample_secs.max(1))).await;
    let snap = collector.snapshot(true).await;

    let out = serde_json::json!({ "runtimes": collector.runtimes(), "snapshot": snap });
    let json = if compact { serde_json::to_string(&out)? } else { serde_json::to_string_pretty(&out)? };
    println!("{json}");
    Ok(())
}
