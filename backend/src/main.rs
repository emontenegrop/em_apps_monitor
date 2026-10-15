use std::{net::SocketAddr, sync::Arc};

use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agents;
mod api;
mod auth;
mod config;
mod db;
mod error;
mod models;
mod scheduler;
mod state;
mod uptime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "em_monitor=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::Config::from_env()?;

    tracing::info!("Connecting to database…");
    let pool = db::create_pool(&cfg.database_url).await?;
    db::run_migrations(&pool).await?;

    auth::bootstrap_admin(&pool, cfg.admin_username.as_deref(), cfg.admin_password.as_deref()).await?;

    let agent_ca = match agents::enroll::AgentCa::load(std::path::Path::new(&cfg.agent_ca_file)) {
        Ok(ca) => {
            tracing::info!("CA para agentes: {} (huella {})", cfg.agent_ca_file, ca.fingerprint);
            Some(ca)
        }
        Err(e) => {
            tracing::warn!("sin CA para agentes ({}: {e}): la inscripción no podrá verificar el central", cfg.agent_ca_file);
            None
        }
    };

    let settings_cipher = match cfg.settings_key.as_deref().map(uptime::crypto::SettingsCipher::from_base64) {
        Some(Ok(c)) => Some(c),
        Some(Err(e)) => {
            tracing::warn!("SETTINGS_KEY inválida ({e}): no se podrá guardar la contraseña SMTP");
            None
        }
        None => {
            tracing::warn!("sin SETTINGS_KEY: no se podrá guardar la contraseña SMTP");
            None
        }
    };

    let state = Arc::new(state::AppState::new(pool, &cfg, agent_ca, settings_cipher));

    tracing::info!(
        "Tareas de fondo: intervalo de agentes {}s, retención {} días",
        cfg.collection_interval_secs,
        cfg.metrics_retention_days
    );
    scheduler::start_background_tasks(state.clone(), cfg.collection_interval_secs, cfg.metrics_retention_days).await;
    uptime::worker::start(state.clone());

    // Sin CORS: la UI se sirve desde el mismo origen (nginx) y la sesión va en cookie SameSite=Strict
    let app = axum::Router::new()
        .nest("/api", api::router(state.clone()))
        .route(
            "/ws",
            axum::routing::get(api::websocket::ws_handler)
                .route_layer(axum::middleware::from_fn_with_state(state.clone(), auth::require_session)),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.server_addr).await?;
    tracing::info!("EM Monitor backend listening on {}", cfg.server_addr);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}
