use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Docker error: {0}")]
    Docker(#[from] bollard::errors::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Agent error: {0}")]
    Agent(#[from] crate::agents::registry::RpcError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Database(e) => {
                tracing::error!("DB error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".into())
            }
            AppError::Docker(e) => {
                tracing::error!("Docker error: {e}");
                (StatusCode::BAD_GATEWAY, format!("Docker error: {e}"))
            }
            AppError::Json(e) => {
                tracing::error!("JSON error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization error: {e}"))
            }
            AppError::Agent(e) => {
                use crate::agents::registry::RpcError;
                let status = match e {
                    RpcError::Offline => StatusCode::SERVICE_UNAVAILABLE,
                    RpcError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
                    RpcError::Agent(_) => StatusCode::BAD_GATEWAY,
                };
                (status, e.to_string())
            }
            AppError::Internal(e) => {
                tracing::error!("Internal error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;
