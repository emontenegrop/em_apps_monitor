//! Ocultamiento de valores sensibles en respuestas de los runtimes.

use serde_json::Value;

pub const REDACTED: &str = "***";

/// Reemplaza los valores de `Config.Env` de un `inspect` por `***`, conservando los
/// nombres (`DB_PASSWORD=***`). Las variables de entorno suelen llevar contraseñas y tokens.
pub fn redact_inspect_env(inspect: &mut Value) {
    let Some(env) = inspect.pointer_mut("/Config/Env").and_then(Value::as_array_mut) else {
        return;
    };
    for var in env.iter_mut() {
        if let Some(s) = var.as_str() {
            let name = s.split_once('=').map_or(s, |(name, _)| name);
            *var = Value::String(format!("{name}={REDACTED}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hides_values_keeps_names() {
        let mut v = json!({
            "Name": "/db",
            "Config": { "Env": ["POSTGRES_PASSWORD=secreto", "PATH=/usr/bin", "SOLO_NOMBRE", "URL=a=b"] }
        });
        redact_inspect_env(&mut v);
        assert_eq!(
            v["Config"]["Env"],
            json!(["POSTGRES_PASSWORD=***", "PATH=***", "SOLO_NOMBRE=***", "URL=***"])
        );
        assert_eq!(v["Name"], "/db");
    }

    #[test]
    fn tolerates_missing_or_null_env() {
        let mut v = json!({ "Config": { "Env": null } });
        redact_inspect_env(&mut v);
        assert_eq!(v, json!({ "Config": { "Env": null } }));
        let mut v = json!({});
        redact_inspect_env(&mut v);
        assert_eq!(v, json!({}));
    }
}
