pub mod containers;
pub mod metrics;
pub mod servers;
pub mod websocket;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};

use crate::{agents, auth, state::AppState, uptime::api as uptime};

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    // Sin sesión: login y endpoints de los agentes (que se autentican con su propia credencial)
    let public = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/agent/ws", get(agents::ws::agent_ws))
        .route("/agent/enroll", post(agents::enroll::enroll))
        .route("/agent/ca.crt", get(agents::enroll::ca_certificate))
        .route("/agent/ca.der", get(agents::enroll::ca_certificate_der));

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
        .route("/metrics/history", get(metrics::host_history))
        .route("/servers/:id/metrics/containers/history", get(metrics::containers_history))
        .route("/servers/:id/metrics/host", get(metrics::host_metrics))
        .route("/servers/:id/metrics/docker", get(metrics::docker_info))
        .route("/servers/:id/metrics/disks", get(metrics::disk_usage))
        .route("/servers/:id/runtimes", get(metrics::runtimes))
        .route("/servers/:id/containers/:cid/metrics", get(metrics::container_metrics))
        // Disponibilidad de URLs/IPs
        .route("/uptime/targets", get(uptime::list_targets).post(uptime::create_target))
        .route("/uptime/targets/:id", delete(uptime::delete_target))
        .route("/uptime/targets/:id/active", patch(uptime::set_target_active))
        .route("/uptime/targets/:id/check", post(uptime::check_target_now))
        .route("/uptime/availability", get(uptime::availability))
        .route("/uptime/response-times", get(uptime::response_times))
        .route("/uptime/incidents", get(uptime::incidents))
        // Notificaciones por correo
        .route("/settings/notifications", get(uptime::get_notifications).put(uptime::save_notifications))
        .route("/settings/notifications/recipients", post(uptime::add_recipient))
        .route("/settings/notifications/recipients/:id", delete(uptime::delete_recipient))
        .route("/settings/notifications/test", post(uptime::test_notifications))
        .route_layer(middleware::from_fn_with_state(state, auth::require_session));

    public.merge(protected)
}
