use crate::{now_ms, protocol::*};
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex};

pub(crate) fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(crate) struct Database(Mutex<Connection>);
impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        ensure!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))? <= 1,
            "База создана более новой версией сервера"
        );
        db.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY, token TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL, can_play INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS invitations(code TEXT PRIMARY KEY, expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS commands(id TEXT PRIMARY KEY, sender TEXT NOT NULL,
                target TEXT NOT NULL, payload TEXT NOT NULL, status TEXT NOT NULL, created INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, device TEXT NOT NULL,
                revision INTEGER NOT NULL, track_key TEXT NOT NULL, title TEXT NOT NULL,
                artist TEXT NOT NULL, started INTEGER NOT NULL, listened INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS session_track ON sessions(track_key);
            CREATE TABLE IF NOT EXISTS connection_vault(name TEXT PRIMARY KEY,clock INTEGER NOT NULL,writer TEXT NOT NULL,encrypted TEXT NOT NULL);
            PRAGMA user_version=1;")?;
        Ok(Self(Mutex::new(db)))
    }
    pub fn vault(&self) -> Result<Vec<crate::vault::Entry>> {
        let db = self.0.lock().unwrap();
        let mut query =
            db.prepare("SELECT name,clock,writer,encrypted FROM connection_vault ORDER BY name")?;
        let rows = query.query_map([], |r| {
            Ok(crate::vault::Entry {
                name: r.get(0)?,
                clock: r.get(1)?,
                writer: r.get(2)?,
                encrypted: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn vault_set(&self, entries: &[crate::vault::Entry]) -> Result<()> {
        ensure!(
            entries.len() <= 4 && entries.iter().all(crate::vault::Entry::valid),
            "Некорректная синхронизация"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        for e in entries {
            tx.execute("INSERT INTO connection_vault VALUES(?1,?2,?3,?4) ON CONFLICT(name) DO UPDATE SET clock=excluded.clock,writer=excluded.writer,encrypted=excluded.encrypted WHERE excluded.clock>connection_vault.clock OR (excluded.clock=connection_vault.clock AND excluded.writer>connection_vault.writer)",params![e.name,e.clock,e.writer,e.encrypted])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn invitation(&self) -> Result<String> {
        let code = secret();
        let db = self.0.lock().unwrap();
        db.execute("DELETE FROM invitations WHERE expires < ?", [now_ms()])?;
        // Bound forgotten invitations even if an administrator repeatedly clicks the button.
        db.execute("DELETE FROM invitations WHERE rowid NOT IN (SELECT rowid FROM invitations ORDER BY rowid DESC LIMIT 15)", [])?;
        db.execute(
            "INSERT INTO invitations VALUES(?1,?2)",
            params![digest(&code), now_ms() + 600_000],
        )?;
        Ok(code)
    }
    pub fn pair(&self, request: &PairRequest) -> Result<Credentials> {
        ensure!(
            !request.name.trim().is_empty()
                && request.name.chars().count() <= 60
                && !request.name.chars().any(char::is_control)
                && request.code.len() <= 128,
            "Некорректное имя устройства или приглашение"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        ensure!(
            tx.execute(
                "DELETE FROM invitations WHERE code=?1 AND expires>=?2",
                params![digest(&request.code), now_ms()]
            )? == 1,
            "Приглашение истекло или уже использовано"
        );
        ensure!(
            tx.query_row("SELECT count(*) FROM devices WHERE revoked=0", [], |r| r
                .get::<_, u32>(0))?
                < 32,
            "Достигнут лимит устройств"
        );
        let credentials = Credentials {
            device_id: uuid::Uuid::new_v4().to_string(),
            token: secret(),
        };
        tx.execute(
            "INSERT INTO devices(id,token,name,can_play) VALUES(?1,?2,?3,?4)",
            params![
                credentials.device_id,
                digest(&credentials.token),
                request.name.trim(),
                request.can_play
            ],
        )?;
        tx.commit()?;
        Ok(credentials)
    }
    pub fn authenticate(&self, token: &str) -> Result<Option<Device>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .query_row(
                "SELECT id,name,can_play FROM devices WHERE token=? AND revoked=0",
                [digest(token)],
                |r| {
                    Ok(Device {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        can_play: r.get(2)?,
                        online: false,
                        updated_ms: 0,
                        playback: Playback::default(),
                    })
                },
            )
            .optional()?)
    }
    pub fn devices(&self) -> Result<Vec<Device>> {
        let db = self.0.lock().unwrap();
        let mut query =
            db.prepare("SELECT id,name,can_play FROM devices WHERE revoked=0 ORDER BY rowid")?;
        let rows = query.query_map([], |r| {
            Ok(Device {
                id: r.get(0)?,
                name: r.get(1)?,
                can_play: r.get(2)?,
                online: false,
                updated_ms: 0,
                playback: Playback::default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .execute("UPDATE devices SET revoked=1 WHERE id=?", [id])?;
        Ok(())
    }
    pub fn command(&self, sender: &str, command: &RemoteCommand) -> Result<(bool, String)> {
        ensure!(
            uuid::Uuid::parse_str(&command.id).is_ok() && command.action.valid(),
            "Некорректная команда"
        );
        let payload = serde_json::to_string(command)?;
        let db = self.0.lock().unwrap();
        let old = db
            .query_row(
                "SELECT sender,payload,status FROM commands WHERE id=?",
                [&command.id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((owner, body, status)) = old {
            ensure!(
                owner == sender && body == payload,
                "Номер команды уже использован"
            );
            return Ok((false, status));
        }
        let target_exists = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM devices WHERE id=? AND can_play=1 AND revoked=0)",
            [&command.target],
            |r| r.get::<_, bool>(0),
        )?;
        ensure!(target_exists, "Устройство не является плеером");
        db.execute(
            "INSERT INTO commands VALUES(?1,?2,?3,?4,'pending',?5)",
            params![command.id, sender, command.target, payload, now_ms()],
        )?;
        // Retain a day of command IDs to reject reconnect replays without unbounded growth.
        db.execute(
            "DELETE FROM commands WHERE created<?",
            [now_ms().saturating_sub(86_400_000)],
        )?;
        Ok((true, "pending".into()))
    }
    pub fn acknowledge(&self, device: &str, id: &str, status: &str) -> Result<Option<String>> {
        let db = self.0.lock().unwrap();
        let sender = db
            .query_row(
                "SELECT sender FROM commands WHERE id=?1 AND target=?2 AND status='pending'",
                params![id, device],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if sender.is_some() {
            db.execute(
                "UPDATE commands SET status=?1 WHERE id=?2 AND target=?3 AND status='pending'",
                params![status, id, device],
            )?;
        }
        Ok(sender)
    }
    pub fn sessions(&self, device: &str, sessions: &[Session]) -> Result<()> {
        ensure!(
            sessions.len() <= 100 && sessions.iter().all(Session::valid),
            "Некорректная статистика"
        );
        let mut db = self.0.lock().unwrap();
        let tx = db.transaction()?;
        for session in sessions {
            let previous = tx
                .query_row(
                    "SELECT device,track_key,started FROM sessions WHERE id=?",
                    [&session.id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, u64>(2)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((owner, key, start)) = previous {
                ensure!(
                    owner == device && key == session.track.key && start == session.started_ms,
                    "Конфликт сессии прослушивания"
                );
            }
            tx.execute(
                "INSERT INTO sessions VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,listened=excluded.listened
                WHERE excluded.revision>sessions.revision AND excluded.listened>=sessions.listened",
                params![
                    session.id,
                    device,
                    session.revision,
                    session.track.key,
                    session.track.title,
                    session.track.artist,
                    session.started_ms,
                    session.listened_ms
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn statistics(&self) -> Result<Statistics> {
        let db = self.0.lock().unwrap();
        let (listened_ms, sessions) = db.query_row(
            "SELECT coalesce(sum(listened),0),count(*) FROM sessions WHERE listened>0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut query = db.prepare(
            "SELECT title,artist,sum(listened) AS total FROM sessions
            WHERE listened>0 GROUP BY track_key ORDER BY total DESC LIMIT 10",
        )?;
        let rows = query.query_map([], |r| {
            Ok(TopTrack {
                title: r.get(0)?,
                artist: r.get(1)?,
                listened_ms: r.get(2)?,
            })
        })?;
        Ok(Statistics {
            listened_ms,
            sessions,
            top_tracks: rows.collect::<rusqlite::Result<_>>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invitations_are_single_use_and_revocation_invalidates_tokens() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        let request = PairRequest {
            code: db.invitation().unwrap(),
            name: "ПК".into(),
            can_play: true,
        };
        let credentials = db.pair(&request).unwrap();
        assert!(db.pair(&request).is_err());
        assert!(db.authenticate(&credentials.token).unwrap().is_some());
        db.revoke(&credentials.device_id).unwrap();
        assert!(db.authenticate(&credentials.token).unwrap().is_none());
    }
    #[test]
    fn statistics_survives_reopen_and_old_or_repeated_batches_do_not_double_count() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("cloud.sqlite3");
        let db = Database::open(&file).unwrap();
        let mut session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            track: Track {
                key: "track-1".into(),
                title: "Трек".into(),
                artist: "Исполнитель".into(),
                duration: 180.,
                ..Default::default()
            },
            started_ms: now_ms(),
            listened_ms: 5000,
        };
        db.sessions("player", &[session.clone()]).unwrap();
        db.sessions("player", &[session.clone()]).unwrap();
        session.revision = 2;
        session.listened_ms = 9000;
        db.sessions("player", &[session.clone()]).unwrap();
        session.revision = 1;
        session.listened_ms = 1000;
        db.sessions("player", &[session.clone()]).unwrap();
        assert!(db.sessions("other", &[session]).is_err());
        drop(db);
        let stats = Database::open(&file).unwrap().statistics().unwrap();
        assert_eq!(stats.listened_ms, 9000);
        assert_eq!(stats.sessions, 1);
        assert_eq!(stats.top_tracks[0].title, "Трек");
    }
    #[test]
    fn duplicate_command_is_not_executed_twice_and_id_collision_is_rejected() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        let target = db
            .pair(&PairRequest {
                code: db.invitation().unwrap(),
                name: "PC".into(),
                can_play: true,
            })
            .unwrap();
        let mut command = RemoteCommand {
            id: uuid::Uuid::new_v4().to_string(),
            target: target.device_id,
            action: Action::Next,
        };
        assert!(db.command("sender", &command).unwrap().0);
        assert!(!db.command("sender", &command).unwrap().0);
        assert!(db
            .acknowledge("other", &command.id, "done")
            .unwrap()
            .is_none());
        assert_eq!(
            db.acknowledge(&command.target, &command.id, "done")
                .unwrap()
                .as_deref(),
            Some("sender")
        );
        assert_eq!(db.command("sender", &command).unwrap().1, "done");
        command.action = Action::Previous;
        assert!(db.command("sender", &command).is_err());
    }
}
