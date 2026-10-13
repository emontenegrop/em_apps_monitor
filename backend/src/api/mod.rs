pub mod containers;
pub mod metrics;
pub mod servers;
pub mod websocket;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};

use crate::{agents, auth, state::AppState};

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    // Sin sesión: login y endpoints de los agentes (que se autentican con su propia credencial)
    let public = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/agent/ws", get(agents::ws::agent_ws))
        .route("/agent/enroll", post(agents::enroll::enroll))
        .route("/agent/ca.crt", get(agents::enroll::ca_certificate));

    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/password", post(auth::change_password))
        // Servers
        .route("/servers", get(servers::list_servers))
        .route("/servers", post(servers::create_server))
        .route("/servers/:id", get(servers::get_server))
        .route("/servers/:id", put(servers::update_server))
        .route("/servers/:id", delete(servers::delete_server))
        .route("/servers/:id/ping", post(servers::ping_server))
        .route("/servers/:id/enrollment-token", post(agents::enroll::create_enrollment_token))
        .route("/servers/:id/revoke", post(agents::enroll::revoke_agent))
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
        .route_layer(middleware::from_fn_with_state(state, auth::require_session));

    public.merge(protected)
}
