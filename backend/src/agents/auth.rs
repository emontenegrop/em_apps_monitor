//! Credenciales de los agentes: `Authorization: Bearer <server_id>.<secreto>`.
//! En la BD solo se guarda el sha256 del secreto.

use axum::http::{header::AUTHORIZATION, HeaderMap};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Secreto aleatorio de 32 bytes en hexadecimal
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("falta la cabecera Authorization")]
    Missing,
    #[error("credencial con formato inválido")]
    Malformed,
    #[error("credencial rechazada")]
    Rejected,
    #[error("error de base de datos: {0}")]
    Database(#[from] sqlx::Error),
}

/// Valida la credencial y devuelve el `server_id` del agente
pub async fn authenticate(db: &PgPool, headers: &HeaderMap) -> Result<Uuid, AuthError> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::Missing)?;
    let token = value.strip_prefix("Bearer ").ok_or(AuthError::Malformed)?;
    let (id, secret) = token.split_once('.').ok_or(AuthError::Malformed)?;
    let server_id = Uuid::parse_str(id).map_err(|_| AuthError::Malformed)?;

    let stored: Option<Option<String>> = sqlx::query_scalar(
        "SELECT agent_secret_hash FROM servers WHERE id = $1 AND is_active = true",
    )
    .bind(server_id)
    .fetch_optional(db)
    .await?;

    // Se comparan hashes de un secreto de 256 bits: el tiempo de comparación no filtra nada útil
    match stored.flatten() {
        Some(hash) if hash == hash_secret(secret) => Ok(server_id),
        _ => Err(AuthError::Rejected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_random_and_hash_is_stable() {
        let a = generate_secret();
        let b = generate_secret();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(hash_secret(&a), hash_secret(&a));
        assert_ne!(hash_secret(&a), hash_secret(&b));
    }
}
