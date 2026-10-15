//! Cifrado de secretos de configuración (contraseña SMTP) con AES-256-GCM.
//! Formato guardado: base64(nonce de 12 bytes || texto cifrado).

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;

pub struct SettingsCipher {
    cipher: Aes256Gcm,
}

impl SettingsCipher {
    /// Clave en base64 de 32 bytes (`openssl rand -base64 32`)
    pub fn from_base64(key: &str) -> anyhow::Result<Self> {
        let bytes = B64.decode(key.trim()).context("SETTINGS_KEY no es base64 válido")?;
        if bytes.len() != 32 {
            return Err(anyhow!("SETTINGS_KEY debe tener 32 bytes (tiene {})", bytes.len()));
        }
        let cipher = Aes256Gcm::new_from_slice(&bytes).map_err(|e| anyhow!("{e}"))?;
        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plain: &str) -> anyhow::Result<String> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
            .map_err(|_| anyhow!("no se pudo cifrar"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(B64.encode(out))
    }

    pub fn decrypt(&self, stored: &str) -> anyhow::Result<String> {
        let raw = B64.decode(stored).context("secreto guardado con formato inválido")?;
        if raw.len() < 13 {
            return Err(anyhow!("secreto guardado con formato inválido"));
        }
        let (nonce, ct) = raw.split_at(12);
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| anyhow!("no se pudo descifrar: ¿cambió SETTINGS_KEY?"))?;
        String::from_utf8(plain).context("secreto descifrado no es UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    #[test]
    fn roundtrip() {
        let c = SettingsCipher::from_base64(KEY).unwrap();
        let enc = c.encrypt("clave secreta").unwrap();
        assert_ne!(enc, "clave secreta");
        assert_eq!(c.decrypt(&enc).unwrap(), "clave secreta");
    }

    #[test]
    fn rejects_short_key() {
        assert!(SettingsCipher::from_base64("AAEC").is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let enc = SettingsCipher::from_base64(KEY).unwrap().encrypt("x").unwrap();
        let other = SettingsCipher::from_base64("HwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        assert!(other.decrypt(&enc).is_err());
    }
}
