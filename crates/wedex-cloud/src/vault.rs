//! Provider connections are encrypted on the device; the relay only merges opaque entries.
use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305,
};
use serde::{Deserialize, Serialize};
pub const FIELDS: [&str; 4] = ["yandex", "lastfm", "soundcloud", "hitmo"];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub clock: u64,
    pub writer: String,
    pub encrypted: String,
}
impl Entry {
    pub fn valid(&self) -> bool {
        FIELDS.contains(&self.name.as_str())
            && self.clock > 0
            && self.clock < i64::MAX as u64
            && uuid::Uuid::parse_str(&self.writer).is_ok()
            && self.encrypted.len() <= 16000
            && self.encrypted.len() >= 54
    }
}
pub fn new_key() -> Result<String> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|_| anyhow::anyhow!("Нет системного генератора ключей"))?;
    Ok(B64.encode(key))
}
pub fn valid_key(key: &str) -> bool {
    B64.decode(key).is_ok_and(|v| v.len() == 32)
}
pub fn seal(key: &str, name: &str, value: &str) -> Result<String> {
    let bytes = B64.decode(key)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Некорректный ключ синхронизации"))?;
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| anyhow::anyhow!("Нет системного генератора nonce"))?;
    let data = cipher
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: value.as_bytes(),
                aad: name.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("Не удалось зашифровать подключение"))?;
    let mut out = nonce.to_vec();
    out.extend(data);
    Ok(B64.encode(out))
}
pub fn open(key: &str, name: &str, value: &str) -> Result<String> {
    let bytes = B64.decode(key)?;
    let data = B64.decode(value)?;
    anyhow::ensure!(data.len() >= 40, "Некорректный пакет синхронизации");
    let cipher = XChaCha20Poly1305::new_from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Некорректный ключ синхронизации"))?;
    let plain = cipher
        .decrypt(
            data[..24].into(),
            Payload {
                msg: &data[24..],
                aad: name.as_bytes(),
            },
        )
        .map_err(|_| {
            anyhow::anyhow!("Ключ приглашения не подходит для синхронизации подключений")
        })?;
    Ok(String::from_utf8(plain)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encryption_binds_field_and_detects_wrong_keys() {
        let key = new_key().unwrap();
        let v = seal(&key, "yandex", "secret-test-token").unwrap();
        assert!(!v.contains("secret-test-token"));
        assert_eq!(open(&key, "yandex", &v).unwrap(), "secret-test-token");
        assert!(open(&key, "lastfm", &v).is_err());
        assert!(open(&new_key().unwrap(), "yandex", &v).is_err());
        assert_ne!(seal(&key, "yandex", "secret-test-token").unwrap(), v);
    }
}
