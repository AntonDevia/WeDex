use super::*;
use wedex_cloud::library::Delta;
pub(super) struct Sync {
    space: Option<String>,
    pub inflight: Option<Instant>,
    pub dirty: bool,
    changed: bool,
    polled: Instant,
    pulled: Instant,
}
impl Default for Sync {
    fn default() -> Self {
        let now = Instant::now() - Duration::from_secs(60);
        Self {
            space: None,
            inflight: None,
            dirty: true,
            changed: false,
            polled: now,
            pulled: now,
        }
    }
}
impl Cloud {
    pub fn take_library_change(&mut self) -> bool {
        std::mem::take(&mut self.library.changed)
    }
    pub(super) fn library_delta(&mut self, store: &Store, delta: &Delta) -> anyhow::Result<()> {
        if let Some(space) = &self.library.space {
            self.library.changed |= store.library_apply(space, delta)?;
            self.library.inflight = None;
            self.library.pulled = Instant::now();
            self.library.dirty |= delta.more;
            self.library.polled = Instant::now() - Duration::from_secs(1);
        }
        Ok(())
    }
    pub(super) fn library_tick(&mut self, store: &Store, now: Instant) -> anyhow::Result<()> {
        let Some(connection) = &self.connection else {
            self.view.library_enabled = false;
            self.view.library_syncing = false;
            return Ok(());
        };
        let space = connection.server_id();
        if self.library.space.as_ref() != Some(&space) {
            store.library_attach(&space)?;
            self.library.space = Some(space.clone());
            self.library.dirty = true;
        }
        self.view.library_enabled = true;
        if self
            .library
            .inflight
            .is_some_and(|at| now.duration_since(at) > Duration::from_secs(8))
        {
            self.library.inflight = None;
            self.library.dirty = true;
        }
        self.view.library_syncing = self.library.inflight.is_some();
        if self.library.inflight.is_some()
            || now.duration_since(self.library.polled) < Duration::from_millis(500)
        {
            return Ok(());
        }
        self.library.polled = now;
        let (cursor, operations) = store.library_request(&space)?;
        let supported = self
            .view
            .capabilities
            .iter()
            .any(|v| v == "library-sync-v1");
        self.view.library_pending = !operations.is_empty() || !supported;
        if !self.view.connected || !supported {
            return Ok(());
        }
        if self.library.dirty
            || !operations.is_empty()
            || now.duration_since(self.library.pulled) > Duration::from_secs(30)
        {
            self.send(ClientMessage::LibrarySync { cursor, operations })?;
            self.library.dirty = false;
            self.library.inflight = Some(now);
            self.view.library_syncing = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tick(cloud: &mut Cloud, store: &Store) {
        let favorites = store.favorites().unwrap();
        cloud.tick(
            LocalPlayback {
                status: &audio::Status::default(),
                queue: &[],
                cursor: None,
                settings: &Settings::default(),
                shuffle: false,
                favorites: &favorites,
            },
            Some(store),
        );
    }
    fn pump(
        a: &mut Cloud,
        sa: &Store,
        b: &mut Cloud,
        sb: &Store,
        done: impl Fn(&Store, &Store) -> bool,
    ) {
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            tick(a, sa);
            tick(b, sb);
            assert!(a.errors.is_empty(), "{:?}", a.errors);
            assert!(b.errors.is_empty(), "{:?}", b.errors);
            if done(sa, sb) {
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        panic!("Collection sync did not converge")
    }
    #[test]
    fn two_real_clients_merge_remove_and_deliver_offline_edits_after_reconnection() {
        let directory =
            std::env::temp_dir().join(format!("wedex-library-{}", uuid::Uuid::new_v4()));
        {
            let server = server::start(server::Config {
                data_dir: directory.clone(),
                bind: "127.0.0.1:0".parse().unwrap(),
            })
            .unwrap();
            let pc = client::blocking_pair(&server.invitation(false).unwrap(), "PC", true).unwrap();
            let phone =
                client::blocking_pair(&server.invitation(false).unwrap(), "Phone", true).unwrap();
            let a = Store::memory_for_test();
            let b = Store::memory_for_test();
            let t = crate::model::tests::track("pc-song", "Artist", 180);
            let u = crate::model::tests::track("phone-song", "Artist", 200);
            a.favorite(&t, true).unwrap();
            b.favorite(&u, true).unwrap();
            let mut ca = Cloud::connected_for_test(pc.clone());
            let mut cb = Cloud::connected_for_test(phone.clone());
            pump(&mut ca, &a, &mut cb, &b, |a, b| {
                a.favorites().unwrap().len() == 2 && b.favorites().unwrap().len() == 2
            });
            b.favorite(&t, false).unwrap();
            pump(&mut ca, &a, &mut cb, &b, |a, b| {
                a.favorites().unwrap().len() == 1
                    && b.library_request(&phone.server_id()).unwrap().1.is_empty()
            });
            cb = Cloud::new(true);
            b.favorite(&t, true).unwrap();
            b.favorite(&u, false).unwrap();
            assert_eq!(b.library_request(&phone.server_id()).unwrap().1.len(), 2);
            tick(&mut cb, &b);
            cb = Cloud::connected_for_test(phone.clone());
            pump(&mut ca, &a, &mut cb, &b, |a, b| {
                a.favorites().unwrap().len() == 1
                    && a.favorites().unwrap()[0].key() == t.key()
                    && b.library_request(&phone.server_id()).unwrap().1.is_empty()
            });
            assert_eq!(
                a.favorites().unwrap()[0].key(),
                b.favorites().unwrap()[0].key()
            );
            assert!(a.library_request(&pc.server_id()).unwrap().1.is_empty());
        }
        assert!(directory.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
