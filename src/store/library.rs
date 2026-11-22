use super::*;
use rusqlite::OptionalExtension;
use wedex_cloud::library::{Delta, Entry, Operation, Track as Metadata, PAGE};

pub(super) fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS library_active(id INTEGER PRIMARY KEY CHECK(id=1),space TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS library_state(space TEXT PRIMARY KEY,cursor INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS library_known(space TEXT NOT NULL,key TEXT NOT NULL,version INTEGER NOT NULL,PRIMARY KEY(space,key));
        CREATE TABLE IF NOT EXISTS library_outbox(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT UNIQUE NOT NULL,space TEXT NOT NULL,key TEXT NOT NULL,data TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS library_pending ON library_outbox(space,seq);
        CREATE TABLE IF NOT EXISTS library_hidden(key TEXT PRIMARY KEY,track TEXT NOT NULL);")?;
    Ok(())
}
fn web(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .filter(|u| {
            matches!(u.scheme(), "https" | "http")
                && u.username().is_empty()
                && u.password().is_none()
        })
        .map(|mut u| {
            u.set_query(None);
            u.set_fragment(None);
            u.to_string()
        })
        .unwrap_or_default()
}
fn metadata(t: &Track) -> Metadata {
    Metadata {
        id: t.id.clone(),
        provider: format!("{:?}", t.provider),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        duration: t.duration,
        cover: t.cover.as_ref().map(|v| web(v)).filter(|v| !v.is_empty()),
        page_url: web(&t.page_url),
        edition: format!("{:?}", t.edition),
    }
}
pub(super) fn hidden(db: &Connection, key: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM library_hidden WHERE key=?)",
        [key],
        |r| r.get(0),
    )?)
}
fn enqueue(db: &Connection, space: &str, track: &Track, liked: bool, seed: bool) -> Result<()> {
    let track = metadata(track);
    anyhow::ensure!(track.valid(), "Некорректные данные трека для синхронизации");
    let key = track.key();
    let base = db
        .query_row(
            "SELECT version FROM library_known WHERE space=? AND key=?",
            params![space, key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let parent = if seed {
        None
    } else {
        db.query_row(
            "SELECT id FROM library_outbox WHERE space=? AND key=? ORDER BY seq DESC LIMIT 1",
            params![space, key],
            |r| r.get(0),
        )
        .optional()?
    };
    let operation = Operation {
        id: uuid::Uuid::new_v4().to_string(),
        track,
        liked,
        base,
        parent,
        seed,
    };
    db.execute(
        "INSERT INTO library_outbox(id,space,key,data) VALUES(?1,?2,?3,?4)",
        params![operation.id, space, key, serde_json::to_string(&operation)?],
    )?;
    Ok(())
}
pub(super) fn enqueue_active(db: &Connection, track: &Track, liked: bool) -> Result<()> {
    if let Some(space) = db
        .query_row("SELECT space FROM library_active WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        enqueue(db, &space, track, liked, false)?;
    }
    Ok(())
}
impl Store {
    /// A bootstrap is recorded once per server identity, never once per reconnect.
    pub fn library_attach(&self, space: &str) -> Result<()> {
        let initial = self.favorites()?;
        let tx = self.db.unchecked_transaction()?;
        if tx.execute(
            "INSERT OR IGNORE INTO library_state(space) VALUES(?)",
            [space],
        )? == 1
        {
            for track in initial.into_iter().rev() {
                enqueue(&tx, space, &track, true, true)?;
                tx.execute(
                    "INSERT OR IGNORE INTO favorites(key,track) VALUES(?1,?2)",
                    params![track.key(), serde_json::to_string(&track)?],
                )?;
            }
        }
        tx.execute("INSERT OR REPLACE INTO library_active VALUES(1,?)", [space])?;
        tx.commit()?;
        Ok(())
    }
    pub fn library_request(&self, space: &str) -> Result<(u64, Vec<Operation>)> {
        let cursor = self.db.query_row(
            "SELECT cursor FROM library_state WHERE space=?",
            [space],
            |r| r.get(0),
        )?;
        let mut q = self
            .db
            .prepare("SELECT data FROM library_outbox WHERE space=? ORDER BY seq LIMIT ?")?;
        let data = q
            .query_map(params![space, PAGE], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut bytes = 0;
        let operations = data
            .into_iter()
            .take_while(|v| {
                bytes += v.len();
                bytes < 180 * 1024
            })
            .map(|v| Ok(serde_json::from_str(&v)?))
            .collect::<Result<_>>()?;
        Ok((cursor, operations))
    }
    pub fn library_apply(&self, space: &str, delta: &Delta) -> Result<bool> {
        let tx = self.db.unchecked_transaction()?;
        let cursor: u64 = tx.query_row(
            "SELECT cursor FROM library_state WHERE space=?",
            [space],
            |r| r.get(0),
        )?;
        if delta.since != cursor {
            return Ok(false);
        }
        anyhow::ensure!(
            delta.cursor >= cursor
                && delta.entries.len() <= PAGE
                && delta.acknowledged.len() <= PAGE,
            "Некорректный ответ медиатеки"
        );
        for ack in &delta.acknowledged {
            tx.execute(
                "DELETE FROM library_outbox WHERE space=? AND id=?",
                params![space, ack.id],
            )?;
        }
        let mut entries = delta
            .entries
            .iter()
            .chain(delta.acknowledged.iter().map(|a| &a.entry))
            .collect::<Vec<_>>();
        entries.sort_by_key(|e| e.version);
        let mut changed = false;
        for e in entries {
            changed |= apply(&tx, space, e)?;
        }
        tx.execute(
            "UPDATE library_state SET cursor=? WHERE space=?",
            params![delta.cursor, space],
        )?;
        tx.commit()?;
        Ok(changed)
    }
}
fn apply(db: &Connection, space: &str, e: &Entry) -> Result<bool> {
    anyhow::ensure!(
        e.track.valid() && e.version > 0,
        "Некорректная запись медиатеки"
    );
    let key = e.track.key();
    let previous: u64 = db
        .query_row(
            "SELECT version FROM library_known WHERE space=? AND key=?",
            params![space, key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    if e.version < previous {
        return Ok(false);
    }
    db.execute(
        "INSERT OR REPLACE INTO library_known VALUES(?1,?2,?3)",
        params![space, key, e.version],
    )?;
    let pending: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM library_outbox WHERE space=? AND key=?)",
        params![space, key],
        |r| r.get(0),
    )?;
    if pending {
        return Ok(false);
    }
    let track: Track = serde_json::from_value(serde_json::to_value(&e.track)?)?;
    if e.liked {
        db.execute("DELETE FROM library_hidden WHERE key=?", [&key])?;
        db.execute("INSERT INTO favorites(key,track,added) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET track=excluded.track,added=excluded.added",params![key,serde_json::to_string(&track)?,e.added])?;
    } else {
        db.execute("DELETE FROM favorites WHERE key=?", [&key])?;
        db.execute("DELETE FROM imported WHERE key=?", [&key])?;
        db.execute(
            "INSERT OR REPLACE INTO library_hidden VALUES(?1,?2)",
            params![key, serde_json::to_string(&track)?],
        )?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
    fn delta(s: &Store, space: &str, liked: bool, version: u64) -> Delta {
        let (cursor, ops) = s.library_request(space).unwrap();
        let entry = Entry {
            track: metadata(&crate::model::tests::track("song", "Artist", 200)),
            liked,
            version,
            added: 1000,
        };
        Delta {
            since: cursor,
            cursor: version,
            more: false,
            entries: vec![entry.clone()],
            acknowledged: ops
                .iter()
                .map(|o| wedex_cloud::library::Ack {
                    id: o.id.clone(),
                    entry: entry.clone(),
                })
                .collect(),
        }
    }
    #[test]
    fn bootstrap_once_ack_is_atomic_and_remote_changes_do_not_echo() {
        let s = store();
        let t = crate::model::tests::track("song", "Artist", 200);
        s.favorite(&t, true).unwrap();
        s.library_attach("server").unwrap();
        let first = s.library_request("server").unwrap().1;
        assert_eq!(first.len(), 1);
        assert!(first[0].seed);
        s.library_attach("server").unwrap();
        assert_eq!(s.library_request("server").unwrap().1[0].id, first[0].id);
        s.library_apply("server", &delta(&s, "server", true, 1))
            .unwrap();
        assert!(s.library_request("server").unwrap().1.is_empty());
        s.library_apply("server", &delta(&s, "server", false, 2))
            .unwrap();
        assert!(s.favorites().unwrap().is_empty());
        assert!(s.library_request("server").unwrap().1.is_empty());
        assert_eq!(s.import_favorites(&[t]).unwrap(), (0, 1));
        assert!(
            s.favorites().unwrap().is_empty(),
            "Import must not resurrect a synchronized deletion"
        );
        assert!(s.library_request("server").unwrap().1.is_empty());
    }
    #[test]
    fn pending_local_change_wins_visually_until_its_ack_and_readd_has_parent() {
        let s = store();
        let t = crate::model::tests::track("song", "Artist", 200);
        s.library_attach("server").unwrap();
        s.favorite(&t, true).unwrap();
        let old = delta(&s, "server", true, 1);
        s.favorite(&t, false).unwrap();
        s.library_apply("server", &old).unwrap();
        assert!(s.favorites().unwrap().is_empty());
        let queued = s.library_request("server").unwrap().1;
        assert_eq!(queued.len(), 1);
        assert!(!queued[0].liked);
        assert!(queued[0].parent.is_some());
        s.library_apply("server", &delta(&s, "server", false, 2))
            .unwrap();
        s.favorite(&t, true).unwrap();
        let op = s.library_request("server").unwrap().1.remove(0);
        assert_eq!(op.base, 2);
        assert!(op.parent.is_none());
    }
    #[test]
    fn restart_preserves_pending_and_failed_import_cannot_erase_cloud_collection() {
        let db = Connection::open_in_memory().unwrap();
        let s = Store::initialize(db).unwrap();
        let t = crate::model::tests::track("song", "Artist", 200);
        s.import_favorites(std::slice::from_ref(&t)).unwrap();
        s.library_attach("a").unwrap();
        s.import_favorites(&[]).unwrap();
        assert_eq!(s.favorites().unwrap().len(), 1);
        let id = s.library_request("a").unwrap().1[0].id.clone();
        let s = Store::initialize(s.db).unwrap();
        assert_eq!(s.library_request("a").unwrap().1[0].id, id);
        s.library_attach("b").unwrap();
        assert_ne!(s.library_request("b").unwrap().1[0].id, id);
        assert_eq!(s.library_request("a").unwrap().1[0].id, id);
    }
    #[test]
    fn wire_metadata_excludes_local_paths_and_request_fits_websocket_limit() {
        let s = store();
        s.library_attach("server").unwrap();
        for i in 0..128 {
            let mut t = crate::model::tests::track(&format!("song-{i}"), "Artist", 200);
            t.cover = Some("file:///phone/private/cover.jpg".into());
            t.page_url = format!("https://mirror.example/song/{i}?secret=not-for-sync");
            s.favorite(&t, true).unwrap();
        }
        let (cursor, ops) = s.library_request("server").unwrap();
        let data = serde_json::to_string(&wedex_cloud::ClientMessage::LibrarySync {
            cursor,
            operations: ops,
        })
        .unwrap();
        assert!(!data.contains("not-for-sync"));
        assert!(!data.contains("file:"));
        assert!(data.len() < 256 * 1024);
    }
}
