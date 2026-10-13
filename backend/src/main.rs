use std::sync::Arc;

use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agents;
mod api;
mod collectors;
mod config;
mod db;
mod error;
mod models;
mod scheduler;
mod state;

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

    let state = Arc::new(state::AppState::new(pool, cfg.collection_interval_secs));

    tracing::info!(
        "Starting collector (interval = {}s, legacy = {})…",
        cfg.collection_interval_secs,
        cfg.legacy_collector
    );
    scheduler::start_collectors(state.clone(), cfg.collection_interval_secs, cfg.legacy_collector).await;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = axum::Router::new()
        .nest("/api", api::router())
        .route("/ws", axum::routing::get(api::websocket::ws_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.server_addr).await?;
    tracing::info!("EM Monitor backend listening on {}", cfg.server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
