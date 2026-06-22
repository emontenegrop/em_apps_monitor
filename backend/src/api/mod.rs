pub mod containers;
pub mod metrics;
pub mod servers;
pub mod websocket;

use std::sync::Arc;

use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Servers
        .route("/servers", get(servers::list_servers))
        .route("/servers", post(servers::create_server))
        .route("/servers/:id", get(servers::get_server))
        .route("/servers/:id", put(servers::update_server))
        .route("/servers/:id", delete(servers::delete_server))
        .route("/servers/:id/ping", post(servers::ping_server))
        // Containers
        .route("/servers/:id/containers", get(containers::list_containers))
        .route("/servers/:id/containers/:cid", get(containers::get_container))
        .route("/servers/:id/containers/:cid/inspect", get(containers::inspect_container))
        .route("/servers/:id/containers/:cid/logs", get(containers::container_logs))
        // Docker resources
        .route("/servers/:id/images", get(containers::server_images))
        .route("/servers/:id/networks", get(containers::server_networks))
        .route("/servers/:id/volumes", get(containers::server_volumes))
        // Metrics
        .route("/servers/:id/metrics/host", get(metrics::host_metrics))
        .route("/servers/:id/metrics/docker", get(metrics::docker_info))
        .route("/servers/:id/metrics/disks", get(metrics::disk_usage))
        .route("/servers/:id/containers/:cid/metrics", get(metrics::container_metrics))
}
