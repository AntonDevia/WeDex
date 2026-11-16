use super::*;
use crate::library::{Ack, Delta, Entry, Operation, PAGE};

fn entry(db: &Connection, key: &str) -> Result<Option<(Entry, u64)>> {
    let row = db
        .query_row(
            "SELECT track,liked,version,removed,added FROM library_items WHERE key=?",
            [key],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, u64>(2)?,
                    r.get::<_, u64>(3)?,
                    r.get::<_, u64>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(track, liked, version, removed, added)| {
        Ok((
            Entry {
                track: serde_json::from_str(&track)?,
                liked,
                version,
                added,
            },
            removed,
        ))
    })
    .transpose()
}
impl Database {
    pub fn library_sync(
        &self,
        device: &str,
        cursor: u64,
        operations: &[Operation],
    ) -> Result<Delta> {
        ensure!(
            operations.len() <= PAGE && operations.iter().all(Operation::valid),
            "Некорректная медиатека"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        let mut revision: u64 =
            tx.query_row("SELECT revision FROM library_clock WHERE id=1", [], |r| {
                r.get(0)
            })?;
        ensure!(cursor <= revision, "Некорректная ревизия медиатеки");
        let mut ids = Vec::new();
        for op in operations {
            let key = op.track.key();
            let hash = digest(&serde_json::to_string(op)?);
            let old = tx
                .query_row(
                    "SELECT device,digest FROM library_operations WHERE id=?",
                    [&op.id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((writer, stored)) = old {
                ensure!(
                    writer == device && stored == hash,
                    "Повторный идентификатор операции"
                );
                ids.push((op.id.clone(), key));
                continue;
            }
            let mut base = op.base;
            if let Some(parent) = &op.parent {
                let causal: u64 = tx.query_row(
                    "SELECT causal FROM library_operations WHERE id=? AND device=? AND key=?",
                    params![parent, device, key],
                    |r| r.get(0),
                )?;
                base = base.max(causal);
            }
            ensure!(base <= revision, "Некорректная версия записи");
            let previous = entry(&tx, &key)?;
            let allowed = previous
                .as_ref()
                .is_none_or(|(e, removed)| base >= *removed && (!op.seed || e.liked));
            let changed = allowed && previous.as_ref().is_none_or(|(e, _)| e.liked != op.liked);
            let mut causal = base;
            if changed {
                revision += 1;
                causal = revision;
                let removed = if op.liked {
                    previous.as_ref().map(|(_, r)| *r).unwrap_or(0)
                } else {
                    revision
                };
                let added = if op.liked {
                    now_ms()
                } else {
                    previous.as_ref().map(|(e, _)| e.added).unwrap_or(0)
                };
                tx.execute("INSERT INTO library_items VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(key) DO UPDATE SET track=excluded.track,liked=excluded.liked,version=excluded.version,removed=excluded.removed,added=excluded.added",params![key,serde_json::to_string(&op.track)?,op.liked,revision,removed,added])?;
            }
            tx.execute(
                "INSERT INTO library_operations VALUES(?1,?2,?3,?4,?5)",
                params![op.id, device, key, hash, causal],
            )?;
            ids.push((op.id.clone(), key));
        }
        tx.execute("UPDATE library_clock SET revision=? WHERE id=1", [revision])?;
        let acknowledged = ids
            .into_iter()
            .map(|(id, key)| {
                Ok(Ack {
                    id,
                    entry: entry(&tx, &key)?
                        .ok_or_else(|| anyhow::anyhow!("Нет записи операции"))?
                        .0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut entries = Vec::new();
        {
            let mut q = tx.prepare(
                "SELECT key FROM library_items WHERE version>? ORDER BY version LIMIT ?",
            )?;
            let keys = q
                .query_map(params![cursor, PAGE + 1], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for key in keys {
                entries.push(entry(&tx, &key)?.unwrap().0);
            }
        }
        let more = entries.len() > PAGE;
        entries.truncate(PAGE);
        let next = if more {
            entries.last().map(|e| e.version).unwrap_or(cursor)
        } else {
            revision
        };
        tx.commit()?;
        Ok(Delta {
            since: cursor,
            cursor: next,
            more,
            entries,
            acknowledged,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn op(key: &str, liked: bool, base: u64) -> Operation {
        Operation {
            id: uuid::Uuid::new_v4().to_string(),
            track: crate::library::Track {
                id: key.into(),
                provider: "Hitmotop".into(),
                title: key.into(),
                artist: "Artist".into(),
                album: String::new(),
                duration: 180,
                cover: None,
                page_url: format!("https://mirror.example/song/{key}"),
                edition: "Unknown".into(),
            },
            liked,
            base,
            parent: None,
            seed: false,
        }
    }
    #[test]
    fn concurrent_remove_wins_in_both_delivery_orders_and_stale_seed_cannot_resurrect() {
        for reverse in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let db = Database::open(&d.path().join("cloud.sqlite3")).unwrap();
            db.library_sync("a", 0, &[op("song", true, 0)]).unwrap();
            let add = op("song", true, 1);
            let remove = op("song", false, 1);
            for (writer, operation) in if reverse {
                [("a", remove), ("b", add)]
            } else {
                [("b", add), ("a", remove)]
            } {
                db.library_sync(writer, 0, &[operation]).unwrap();
            }
            let mut seed = op("song", true, 0);
            seed.seed = true;
            let state = db.library_sync("new-device", 0, &[seed]).unwrap();
            assert!(!state.entries[0].liked);
            let version = state.entries[0].version;
            db.library_sync("a", 0, &[op("song", true, version)])
                .unwrap();
            db.library_sync("offline", 0, &[op("song", false, 1)])
                .unwrap();
            assert!(
                db.library_sync("a", 0, &[]).unwrap().entries[0].liked,
                "Old removal must not erase an observed re-add"
            );
        }
    }
    #[test]
    fn offline_toggle_chain_is_causal_retry_safe_and_survives_restart() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("cloud.sqlite3");
        let add = op("song", true, 0);
        let mut remove = op("song", false, 0);
        remove.parent = Some(add.id.clone());
        let mut again = op("song", true, 0);
        again.parent = Some(remove.id.clone());
        let operations = [add, remove, again];
        {
            let db = Database::open(&path).unwrap();
            let delta = db.library_sync("a", 0, &operations).unwrap();
            assert!(delta.entries[0].liked);
            assert_eq!(delta.cursor, 3);
        }
        let db = Database::open(&path).unwrap();
        let delta = db.library_sync("a", 0, &operations).unwrap();
        assert_eq!(delta.cursor, 3);
        assert_eq!(delta.acknowledged.len(), 3);
        assert!(db.library_sync("other-device", 0, &operations).is_err());
        let mut forged = operations[0].clone();
        forged.liked = false;
        assert!(db.library_sync("a", 0, &[forged]).is_err());
    }
    #[test]
    fn a_rejected_parent_does_not_grant_knowledge_of_an_unseen_removal() {
        let d = tempfile::tempdir().unwrap();
        let db = Database::open(&d.path().join("db")).unwrap();
        db.library_sync("a", 0, &[op("song", true, 0), op("song", false, 0)])
            .unwrap();
        let rejected = op("song", false, 0);
        let mut child = op("song", true, 0);
        child.parent = Some(rejected.id.clone());
        assert!(!db.library_sync("b", 0, &[rejected, child]).unwrap().entries[0].liked);
    }
    #[test]
    fn pagination_keeps_tombstones_and_validation_rolls_back_the_whole_batch() {
        let d = tempfile::tempdir().unwrap();
        let db = Database::open(&d.path().join("db")).unwrap();
        for start in [0, PAGE] {
            let ops = (start..start + PAGE)
                .map(|i| op(&format!("song-{i}"), i % 2 == 0, 0))
                .collect::<Vec<_>>();
            db.library_sync("a", 0, &ops).unwrap();
        }
        let first = db.library_sync("a", 0, &[]).unwrap();
        assert!(first.more);
        assert_eq!(first.entries.len(), PAGE);
        let second = db.library_sync("a", first.cursor, &[]).unwrap();
        assert!(!second.more);
        assert_eq!(second.entries.len(), PAGE);
        assert!(second.entries.iter().any(|e| !e.liked));
        let mut invalid = op("bad", true, 0);
        invalid.track.page_url = "file:///private/music.mp3".into();
        assert!(db
            .library_sync("a", 0, &[op("never", true, 0), invalid])
            .is_err());
        assert_eq!(
            db.library_sync("a", second.cursor, &[]).unwrap().cursor,
            second.cursor
        );
    }
}
