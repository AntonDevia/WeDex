//! Global identities and routing only. Every account's personal data lives in its own SQLite file.
use crate::{
    database::{digest, secret},
    now_ms, vault, Credentials,
};
use anyhow::{ensure, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
use zeroize::Zeroizing;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub username: String,
}
#[derive(Serialize, Deserialize)]
pub struct Login {
    pub username: String,
    pub password: Zeroizing<String>,
    pub name: String,
    /// Only used while claiming the pre-account installation with an authenticated legacy device.
    #[serde(default)]
    pub sync_key: Option<Zeroizing<String>>,
}
impl Login {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.username.len() >= 3
                && self.username.len() <= 64
                && self
                    .username
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)),
            "Логин: 3–64 латинских символа, цифры, _, . или -"
        );
        ensure!(
            self.password.chars().count() >= 10 && self.password.len() <= 256,
            "Пароль: минимум 10 символов, максимум 256 байт"
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 60
                && !self.name.chars().any(char::is_control),
            "Некорректное имя устройства"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
pub struct Authenticated {
    pub credentials: Credentials,
    pub sync_key: String,
}

pub(crate) struct Registry(Mutex<Connection>);
fn derive(password: &str, salt: &str) -> Result<Zeroizing<String>> {
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt.as_bytes(), key.as_mut())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Zeroizing::new(B64.encode(key.as_ref())))
}
impl Registry {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS accounts(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,salt TEXT NOT NULL,wrapped TEXT NOT NULL,created INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS account_devices(token TEXT PRIMARY KEY,account TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS account_invites(code TEXT PRIMARY KEY,account TEXT NOT NULL,expires INTEGER NOT NULL);")?;
        Ok(Self(Mutex::new(db)))
    }
    pub fn register(&self, request: &Login, legacy: bool) -> Result<(Account, String)> {
        request.validate()?;
        let username = request.username.to_ascii_lowercase();
        let account = Account {
            id: if legacy {
                "legacy".into()
            } else {
                uuid::Uuid::new_v4().to_string()
            },
            username,
        };
        let salt = secret();
        let key = derive(&request.password, &salt)?;
        let sync = if legacy {
            request
                .sync_key
                .as_deref()
                .filter(|k| vault::valid_key(k))
                .map(|k| k.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Для переноса подключений нужно приглашение с ключом синхронизации"
                    )
                })?
        } else {
            vault::new_key()?
        };
        let wrapped = vault::seal(&key, &account.id, &sync)?;
        self.0.lock().unwrap().execute(
            "INSERT INTO accounts VALUES(?1,?2,?3,?4,?5)",
            params![account.id, account.username, salt, wrapped, now_ms()],
        )?;
        Ok((account, sync))
    }
    pub fn login(&self, request: &Login) -> Result<(Account, String)> {
        request.validate()?;
        let row = self
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT id,salt,wrapped FROM accounts WHERE username=?",
                [request.username.to_ascii_lowercase()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        // Unknown users pay the same memory-hard work. No password or reusable raw key is stored.
        let (id, salt, wrapped) = row.unwrap_or_else(|| (String::new(), secret(), String::new()));
        let key = derive(&request.password, &salt)?;
        let sync = vault::open(&key, &id, &wrapped)?;
        Ok((
            Account {
                id,
                username: request.username.to_ascii_lowercase(),
            },
            sync,
        ))
    }
    pub fn account(&self, id: &str) -> Result<Option<Account>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .query_row("SELECT id,username FROM accounts WHERE id=?", [id], |r| {
                Ok(Account {
                    id: r.get(0)?,
                    username: r.get(1)?,
                })
            })
            .optional()?)
    }
    pub fn route_token(&self, token: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT account FROM account_devices WHERE token=?",
                [digest(token)],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn device(&self, token: &str, account: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO account_devices VALUES(?1,?2)",
            params![digest(token), account],
        )?;
        Ok(())
    }
    pub fn invite(&self, code: &str, account: &str) -> Result<()> {
        let db = self.0.lock().unwrap();
        db.execute("DELETE FROM account_invites WHERE expires<?", [now_ms()])?;
        db.execute(
            "INSERT INTO account_invites VALUES(?1,?2,?3)",
            params![digest(code), account, now_ms() + 600_000],
        )?;
        Ok(())
    }
    pub fn route_invite(&self, code: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT account FROM account_invites WHERE code=?1 AND expires>=?2",
                params![digest(code), now_ms()],
                |r| r.get(0),
            )
            .optional()?)
    }
}
