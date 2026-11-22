use crate::model::Track;
#[cfg(not(target_os = "android"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(target_os = "android"))]
use directories::ProjectDirs;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
mod library;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub cache_auto: bool,
    pub cache_limit_mb: u32,
    pub offline_only: bool,
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
    pub recent_wave_ids: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            cache_auto: true,
            cache_limit_mb: 2048,
            offline_only: false,
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
            recent_wave_ids: Vec::new(),
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
fn raw_secret(name: &str) -> String {
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
fn raw_save_secret(name: &str, value: &str) -> Result<()> {
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

fn active_profile() -> String {
    let cloud = raw_secret("cloud");
    if !cloud.is_empty() {
        return serde_json::from_str::<wedex_cloud::client::Connection>(&cloud)
            .ok()
            .filter(|c| c.credentials.account.is_some())
            .map(|c| c.server_id())
            .unwrap_or_default();
    }
    raw_secret("cloud-profile")
}
pub fn secret(name: &str) -> String {
    let profile = active_profile();
    if !profile.is_empty() && matches!(name, "yandex" | "lastfm" | "soundcloud") {
        return raw_secret(&format!("profile:{profile}:{name}"));
    }
    raw_secret(name)
}
pub fn save_secret(name: &str, value: &str) -> Result<()> {
    let profile = active_profile();
    if !profile.is_empty() && matches!(name, "yandex" | "lastfm" | "soundcloud") {
        return raw_save_secret(&format!("profile:{profile}:{name}"), value);
    }
    raw_save_secret(name, value)
}
pub fn remember_profile() -> Result<()> {
    let profile = active_profile();
    if profile.is_empty() {
        Ok(())
    } else {
        raw_save_secret("cloud-profile", &profile)
    }
}
fn profile_directory() -> PathBuf {
    let profile = active_profile();
    if profile.len() == 64 && profile.bytes().all(|b| b.is_ascii_hexdigit()) {
        data_dir().join("profiles").join(profile)
    } else {
        data_dir()
    }
}
fn prepare_profile(
    root: &std::path::Path,
    destination: &std::path::Path,
    import: bool,
) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let source = root.join("library.sqlite3");
    let target = destination.join("library.sqlite3");
    if import && source.exists() && !target.exists() {
        Connection::open(source)?.execute("VACUUM INTO ?", [target.to_string_lossy().as_ref()])?;
    }
    Ok(())
}
/// Import the pre-account installation once. Subsequent accounts start with a separate local library and credentials.
#[cfg(not(test))]
pub fn activate_account(connection: &wedex_cloud::client::Connection) -> Result<bool> {
    if connection.credentials.account.is_none() {
        return Ok(!active_profile().is_empty());
    }
    let scope = connection.server_id();
    if active_profile() == scope {
        return Ok(false);
    }
    let destination = data_dir().join("profiles").join(&scope);
    let first = raw_secret("cloud-profile-adopted").is_empty();
    prepare_profile(&data_dir(), &destination, first)?;
    if first {
        for name in ["yandex", "lastfm", "soundcloud"] {
            let value = raw_secret(name);
            if !value.is_empty() {
                raw_save_secret(&format!("profile:{scope}:{name}"), &value)?;
            }
        }
        raw_save_secret("cloud-profile-adopted", &scope)?;
    }
    // Saving the Connection in one system-credential entry commits both identity and active profile.
    Ok(true)
}

pub struct Store {
    db: Connection,
}
impl Store {
    #[cfg(test)]
    pub(crate) fn memory_for_test() -> Self {
        Self::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
    pub fn open() -> Result<Self> {
        let directory = profile_directory();
        std::fs::create_dir_all(&directory)?;
        let db = Connection::open(directory.join("library.sqlite3"))?;
        Self::initialize(db)
    }
    fn initialize(db: Connection) -> Result<Self> {
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS favorites (key TEXT PRIMARY KEY, track TEXT NOT NULL, added INTEGER NOT NULL DEFAULT (unixepoch())); CREATE TABLE IF NOT EXISTS imported (key TEXT PRIMARY KEY, track TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history (id INTEGER PRIMARY KEY, track TEXT NOT NULL, played INTEGER NOT NULL DEFAULT(unixepoch())); CREATE TABLE IF NOT EXISTS replacements (original TEXT PRIMARY KEY, track TEXT NOT NULL);")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS listening_sessions(id TEXT PRIMARY KEY,revision INTEGER NOT NULL,data TEXT NOT NULL,listened INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS cloud_session_acks(server TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(server,id));")?;
        library::initialize(&db)?;
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
            .prepare("SELECT track FROM (SELECT key,track,added FROM favorites UNION ALL SELECT key,track,0 FROM imported WHERE key NOT IN (SELECT key FROM favorites)) WHERE key NOT IN (SELECT key FROM library_hidden) ORDER BY CASE WHEN added<100000000000 THEN added*1000 ELSE added END DESC,key")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
    }
    pub fn favorite(&self, track: &Track, add: bool) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        if add {
            Self::add_favorite(&tx, track)?;
        } else {
            tx.execute(
                "INSERT OR REPLACE INTO library_hidden VALUES(?1,?2)",
                params![track.key(), serde_json::to_string(track)?],
            )?;
            tx.execute("DELETE FROM favorites WHERE key=?1", [track.key()])?;
            tx.execute("DELETE FROM imported WHERE key=?1", [track.key()])?;
            library::enqueue_active(&tx, track, false)?;
        }
        tx.commit()?;
        Ok(())
    }
    fn add_favorite(tx: &Connection, track: &Track) -> Result<()> {
        tx.execute("DELETE FROM library_hidden WHERE key=?", [track.key()])?;
        tx.execute(
            "INSERT OR REPLACE INTO favorites(key,track,added) VALUES(?1,?2,?3)",
            params![
                track.key(),
                serde_json::to_string(track)?,
                wedex_cloud::now_ms()
            ],
        )?;
        library::enqueue_active(tx, track, true)?;
        Ok(())
    }
    pub fn import_favorites(&self, tracks: &[Track]) -> Result<(usize, usize)> {
        let tx = self.db.unchecked_transaction()?;
        let favorites = self.favorites()?;
        let mut identities: std::collections::HashSet<_> =
            favorites.iter().map(Track::identity).collect();
        let mut keys: std::collections::HashSet<_> = favorites.iter().map(Track::key).collect();
        let mut added = 0;
        let mut hidden = 0;
        for track in tracks {
            let identity = track.identity();
            let key = track.key();
            if identities.contains(&identity) || keys.contains(&key) {
                continue;
            }
            if library::hidden(&tx, &key)? {
                hidden += 1;
            } else {
                Self::add_favorite(&tx, track)?;
                identities.insert(identity);
                keys.insert(key);
                added += 1;
            }
        }
        tx.commit()?;
        Ok((added, hidden))
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
    fn importing_favorites_preserves_legacy_rows_and_deduplicates_between_catalogs() {
        let store = Store::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let a = track("a", "artist", 200);
        let b = track("b", "artist", 200);
        store
            .db
            .execute(
                "INSERT INTO imported VALUES (?1,?2)",
                params![a.key(), serde_json::to_string(&a).unwrap()],
            )
            .unwrap();
        let mut duplicate = a;
        duplicate.provider = crate::model::Provider::YouTube;
        assert_eq!(
            store.import_favorites(&[duplicate, b.clone()]).unwrap(),
            (1, 0)
        );
        assert_eq!(store.favorites().unwrap().len(), 2);
        assert_eq!(store.import_favorites(&[b]).unwrap(), (0, 0));
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

#[cfg(test)]
mod account_profile_tests {
    use super::*;
    #[test]
    fn account_profiles_adopt_existing_music_once_and_keep_later_accounts_separate() {
        let root =
            std::env::temp_dir().join(format!("wedex-profile-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let original =
            Store::initialize(Connection::open(root.join("library.sqlite3")).unwrap()).unwrap();
        let song = crate::model::tests::track("First account only", "Artist", 180);
        original.favorite(&song, true).unwrap();
        let a = root.join("profiles/a");
        let b = root.join("profiles/b");
        prepare_profile(&root, &a, true).unwrap();
        prepare_profile(&root, &b, false).unwrap();
        let first =
            Store::initialize(Connection::open(a.join("library.sqlite3")).unwrap()).unwrap();
        let second =
            Store::initialize(Connection::open(b.join("library.sqlite3")).unwrap()).unwrap();
        assert_eq!(first.favorites().unwrap().len(), 1);
        assert!(second.favorites().unwrap().is_empty());
        second
            .favorite(
                &crate::model::tests::track("Second account only", "Other", 180),
                true,
            )
            .unwrap();
        assert_eq!(first.favorites().unwrap()[0].title, "First account only");
        assert_eq!(original.favorites().unwrap()[0].title, "First account only");
        drop(first);
        drop(second);
        drop(original);
        // Only this UUID-named test directory, resolved under the system temporary directory.
        assert!(
            root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("wedex-profile-test-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
