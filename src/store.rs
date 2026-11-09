use crate::model::Track;
#[cfg(not(target_os = "android"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(target_os = "android"))]
use directories::ProjectDirs;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hitmo_base: String,
    pub theme: ThemePreference,
    pub volume: f32,
    pub eq: [f32; 10],
    pub eq_preamp: f32,
    pub eq_preset: String,
    pub crossfade: f32,
    pub close_to_tray: bool,
    pub lyrics_enabled: bool,
    pub repeat_current: bool,
    pub prefer_original: bool,
    pub discovery: f32,
    pub last_artist: String,
    pub last_title: String,
    pub recent_searches: Vec<Track>,
    pub recent_recommendations: Vec<String>,
    pub recent_radio_seeds: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            hitmo_base: crate::providers::connections::DEFAULT_HITMO.into(),
            theme: ThemePreference::System,
            volume: 0.65,
            eq: [0.0; 10],
            eq_preamp: 0.,
            eq_preset: "По умолчанию".into(),
            crossfade: 0.,
            close_to_tray: true,
            lyrics_enabled: false,
            repeat_current: false,
            prefer_original: true,
            discovery: 0.45,
            last_artist: String::new(),
            last_title: String::new(),
            recent_searches: Vec::new(),
            recent_recommendations: Vec::new(),
            recent_radio_seeds: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}
impl ThemePreference {
    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            Self::System => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        crate::android::data_dir()
    }
    #[cfg(not(target_os = "android"))]
    ProjectDirs::from("app", "WeDex", "Sonora")
        .map(|p| p.data_local_dir().to_owned())
        .unwrap_or_else(|| PathBuf::from(".local"))
}
pub fn secret(name: &str) -> String {
    #[cfg(target_os = "android")]
    {
        crate::android::read_secret(name).unwrap_or_default()
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(value) = std::env::var(format!("WEDEX_{}", name.to_uppercase())) {
            return value;
        }
        let stored = keyring::Entry::new("WeDex.Sonora", name)
            .and_then(|e| e.get_password())
            .unwrap_or_default();
        if !stored.is_empty() {
            return stored;
        }
        // Development-only bootstrap. Never compiled into the executable.
        if name == "lastfm" {
            if let Ok(value) = std::fs::read_to_string(".local/lastfm.key") {
                let value = value.trim().to_owned();
                let _ = save_secret(name, &value);
                return value;
            }
        }
        String::new()
    }
}
pub fn save_secret(name: &str, value: &str) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        crate::android::write_secret(name, value)
    }
    #[cfg(not(target_os = "android"))]
    {
        let entry = keyring::Entry::new("WeDex.Sonora", name)?;
        if value.trim().is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.into()),
            }
        } else {
            entry
                .set_password(value.trim())
                .context("Не удалось сохранить ключ в системном хранилище")
        }
    }
}

pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open() -> Result<Self> {
        std::fs::create_dir_all(data_dir())?;
        let db = Connection::open(data_dir().join("library.sqlite3"))?;
        Self::initialize(db)
    }
    fn initialize(db: Connection) -> Result<Self> {
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS favorites (key TEXT PRIMARY KEY, track TEXT NOT NULL, added INTEGER NOT NULL DEFAULT (unixepoch())); CREATE TABLE IF NOT EXISTS imported (key TEXT PRIMARY KEY, track TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history (id INTEGER PRIMARY KEY, track TEXT NOT NULL, played INTEGER NOT NULL DEFAULT(unixepoch())); CREATE TABLE IF NOT EXISTS replacements (original TEXT PRIMARY KEY, track TEXT NOT NULL);")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS listening_sessions(id TEXT PRIMARY KEY,revision INTEGER NOT NULL,data TEXT NOT NULL,listened INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS cloud_session_acks(server TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(server,id));")?;
        Ok(Self { db })
    }
    pub fn settings(&self) -> Settings {
        self.db
            .query_row("SELECT data FROM settings WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save_session(&self, session: &wedex_cloud::Session) -> Result<()> {
        self.db.execute("INSERT INTO listening_sessions VALUES(?1,?2,?3,?4)
            ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,data=excluded.data,listened=excluded.listened
            WHERE excluded.revision>listening_sessions.revision",
            params![session.id, session.revision, serde_json::to_string(session)?, session.listened_ms])?;
        Ok(())
    }
    pub fn pending_sessions(&self, server: &str) -> Result<Vec<wedex_cloud::Session>> {
        let mut statement = self.db.prepare(
            "SELECT s.data FROM listening_sessions s
            LEFT JOIN cloud_session_acks a ON a.id=s.id AND a.server=?1
            WHERE s.listened>0 AND (a.revision IS NULL OR a.revision<s.revision)
            ORDER BY s.rowid LIMIT 100",
        )?;
        let rows = statement.query_map([server], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn acknowledge_sessions(&self, server: &str, sessions: &[(String, u64)]) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        for (id, revision) in sessions {
            tx.execute("INSERT INTO cloud_session_acks VALUES(?1,?2,?3)
                ON CONFLICT(server,id) DO UPDATE SET revision=max(cloud_session_acks.revision,excluded.revision)",
                params![server,id,revision])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn listened_ms(&self) -> Result<u64> {
        Ok(self.db.query_row(
            "SELECT coalesce(sum(listened),0) FROM listening_sessions",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO settings VALUES(1,?1)",
            [serde_json::to_string(settings)?],
        )?;
        Ok(())
    }
    pub fn favorites(&self) -> Result<Vec<Track>> {
        let mut stmt = self
            .db
            .prepare("SELECT track FROM favorites UNION ALL SELECT track FROM imported WHERE key NOT IN (SELECT key FROM favorites)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
    }
    pub fn favorite(&self, track: &Track, add: bool) -> Result<()> {
        if add {
            self.db.execute(
                "INSERT OR REPLACE INTO favorites(key,track) VALUES(?1,?2)",
                params![track.key(), serde_json::to_string(track)?],
            )?;
        } else {
            self.db
                .execute("DELETE FROM favorites WHERE key=?1", [track.key()])?;
            self.db
                .execute("DELETE FROM imported WHERE key=?1", [track.key()])?;
        }
        Ok(())
    }
    pub fn import_yandex(&self, tracks: &[Track]) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM imported", [])?;
        for track in tracks {
            tx.execute(
                "INSERT OR REPLACE INTO imported VALUES(?1,?2)",
                params![track.key(), serde_json::to_string(track)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn history(&self) -> Result<Vec<Track>> {
        let mut stmt = self
            .db
            .prepare("SELECT track FROM history ORDER BY id DESC LIMIT 100")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
    }
    pub fn played(&self, track: &Track) -> Result<()> {
        self.db.execute(
            "INSERT INTO history(track) VALUES(?1)",
            [serde_json::to_string(track)?],
        )?;
        self.db.execute("DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT 500)", [])?;
        Ok(())
    }
    pub fn replacement(&self, track: &Track) -> Option<Track> {
        self.db
            .query_row(
                "SELECT track FROM replacements WHERE original=?1",
                [track.identity()],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }
    pub fn replace(&self, original: &Track, replacement: &Track) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO replacements VALUES(?1,?2)",
            params![original.identity(), serde_json::to_string(replacement)?],
        )?;
        Ok(())
    }
    pub fn clear_replacement(&self, original: &Track) -> Result<()> {
        self.db.execute(
            "DELETE FROM replacements WHERE original=?1",
            [original.identity()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listening_outbox_acknowledges_only_that_revision_and_that_server() {
        let store = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let mut session = wedex_cloud::Session {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            track: wedex_cloud::Track {
                key: "song".into(),
                title: "Песня".into(),
                artist: "Автор".into(),
                duration: 200.,
                ..Default::default()
            },
            started_ms: wedex_cloud::now_ms(),
            listened_ms: 1000,
        };
        store.save_session(&session).unwrap();
        assert_eq!(store.pending_sessions("server-a").unwrap().len(), 1);
        store
            .acknowledge_sessions("server-a", &[(session.id.clone(), 1)])
            .unwrap();
        assert!(store.pending_sessions("server-a").unwrap().is_empty());
        assert_eq!(store.pending_sessions("server-b").unwrap().len(), 1);
        session.revision = 2;
        session.listened_ms = 2500;
        store.save_session(&session).unwrap();
        store
            .acknowledge_sessions("server-a", &[(session.id.clone(), 1)])
            .unwrap();
        assert_eq!(store.pending_sessions("server-a").unwrap()[0].revision, 2);
        assert_eq!(store.listened_ms().unwrap(), 2500);
    }
    #[test]
    fn theme_is_backward_compatible_and_persists_with_settings() {
        let old: Settings = serde_json::from_str(r#"{"volume":0.8}"#).unwrap();
        assert_eq!(old.theme, ThemePreference::System);
        assert_eq!(old.volume, 0.8);
        assert!(old.close_to_tray);
        assert!(!old.lyrics_enabled);
        assert_eq!(old.crossfade, 0.);
        assert_eq!(old.hitmo_base, crate::providers::connections::DEFAULT_HITMO);
        let store = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let settings = Settings {
            theme: ThemePreference::Dark,
            lyrics_enabled: true,
            crossfade: 4.5,
            close_to_tray: false,
            eq_preamp: -3.,
            hitmo_base: "https://mirror.example/".into(),
            ..Default::default()
        };
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().theme, ThemePreference::Dark);
        let saved = store.settings();
        assert!(saved.lyrics_enabled);
        assert_eq!(saved.crossfade, 4.5);
        assert!(!saved.close_to_tray);
        assert_eq!(saved.eq_preamp, -3.);
        assert_eq!(saved.hitmo_base, "https://mirror.example/");
    }
    use crate::model::tests::track;
    #[test]
    fn sync_replaces_snapshot_preserving_local_favorites() {
        let store = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let a = track("a", "artist", 200);
        let b = track("b", "artist", 200);
        store.import_yandex(std::slice::from_ref(&a)).unwrap();
        store.favorite(&b, true).unwrap();
        assert_eq!(store.favorites().unwrap().len(), 2);
        store.import_yandex(&[]).unwrap();
        assert_eq!(store.favorites().unwrap()[0].id, "b");
    }
    #[test]
    fn replacements_and_history_survive_roundtrip() {
        let store = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let a = track("a", "artist", 200);
        let mut b = a.clone();
        b.id = "replacement".into();
        store.replace(&a, &b).unwrap();
        assert_eq!(store.replacement(&a).unwrap().id, "replacement");
        store.played(&a).unwrap();
        assert_eq!(store.history().unwrap().len(), 1);
        store.clear_replacement(&a).unwrap();
        assert!(store.replacement(&a).is_none());
    }
}
