use crate::{
    cache::{self, View},
    model::Track,
    providers::Services,
};
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

enum Event {
    Listed(u64, Vec<cache::Entry>),
    Downloaded(Arc<AtomicU64>, Result<(), String>, bool),
    Changed(Result<(), String>),
}
pub struct Manager {
    pub view: View,
    pub errors: Vec<String>,
    queue: VecDeque<Track>,
    active: Option<(String, Arc<AtomicU64>)>,
    listing: bool,
    last: Instant,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    preview: bool,
    dirty: bool,
    listed_revision: u64,
}
impl Manager {
    pub fn new(preview: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            view: View::default(),
            errors: vec![],
            queue: VecDeque::new(),
            active: None,
            listing: false,
            last: Instant::now() - Duration::from_secs(10),
            tx,
            rx,
            preview,
            dirty: true,
            listed_revision: u64::MAX,
        }
    }
    pub fn download(&mut self, tracks: Vec<Track>) {
        let mut queued: HashSet<_> = self
            .queue
            .iter()
            .map(Track::key)
            .chain(self.active.iter().map(|(k, _)| k.clone()))
            .collect();
        let saved: HashSet<_> = self
            .view
            .entries
            .iter()
            .filter(|e| e.pinned && self.listed_revision == cache::revision())
            .map(|e| e.track.key())
            .collect();
        for track in tracks {
            let key = track.key();
            if !saved.contains(&key) && queued.insert(key) {
                self.queue.push_back(track);
            }
        }
        self.refresh_pending();
    }
    pub fn busy(&self) -> bool {
        self.active.is_some() || !self.queue.is_empty()
    }
    fn refresh_pending(&mut self) {
        self.view.downloading = Arc::new(
            self.active
                .iter()
                .map(|(k, _)| k.clone())
                .chain(self.queue.iter().map(Track::key))
                .collect(),
        );
        self.view.busy = self.busy();
        self.dirty = false;
    }
    pub fn cancel(&mut self) {
        self.queue.clear();
        if let Some((_, token)) = self.active.take() {
            token.store(2, Ordering::Relaxed);
        }
        self.dirty = true;
        self.view.downloading = Arc::default();
        self.view.busy = false;
    }
    pub fn remove(&mut self, track: Option<Track>) {
        if let Some(track) = &track {
            let key = track.key();
            self.queue.retain(|t| t.key() != key);
            if self.active.as_ref().is_some_and(|(k, _)| k == &key) {
                if let Some((_, token)) = self.active.take() {
                    token.store(2, Ordering::Relaxed);
                }
            }
            self.view.entries = Arc::new(
                self.view
                    .entries
                    .iter()
                    .filter(|e| e.track.key() != key)
                    .cloned()
                    .collect(),
            );
            self.view.bytes = self.view.entries.iter().map(|e| e.size).sum();
            self.listed_revision = u64::MAX;
            self.refresh_pending();
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let cache = cache::global();
            let r = if let Some(t) = track {
                cache.remove(&t)
            } else {
                cache.clear_temporary()
            };
            let _ = tx.send(Event::Changed(r.map_err(|e| e.to_string())));
        });
    }
    pub fn tick(&mut self, services: &Services) {
        if self.preview {
            return;
        }
        while let Ok(e) = self.rx.try_recv() {
            match e {
                Event::Listed(revision, entries) => {
                    self.listing = false;
                    if revision != cache::revision() {
                        self.last = Instant::now() - Duration::from_secs(10);
                        continue;
                    }
                    self.listed_revision = revision;
                    self.view.bytes = entries.iter().map(|e| e.size).sum();
                    self.view.entries = Arc::new(entries);
                }
                Event::Downloaded(token, result, storage_full) => {
                    self.dirty = true;
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|(_, t)| Arc::ptr_eq(t, &token))
                    {
                        self.active = None;
                        if let Err(e) = result {
                            if storage_full {
                                self.queue.clear();
                            }
                            self.errors.push(e);
                        }
                    }
                    self.last = Instant::now() - Duration::from_secs(10);
                }
                Event::Changed(result) => {
                    if let Err(e) = result {
                        self.errors.push(e);
                    }
                    self.last = Instant::now() - Duration::from_secs(10);
                }
            }
        }
        if self.active.is_none() && crate::network::state() != crate::network::State::Offline {
            if let Some(track) = self.queue.pop_front() {
                self.dirty = true;
                let tx = self.tx.clone();
                let service = services.clone();
                let key = track.key();
                let token = Arc::new(AtomicU64::new(1));
                self.active = Some((key.clone(), token.clone()));
                std::thread::spawn(move || {
                    let result = crate::audio::download_offline(&track, &service, token.clone());
                    let storage_full = result.as_ref().err().is_some_and(|e| {
                        e.chain().any(|cause| {
                            cause
                                .downcast_ref::<std::io::Error>()
                                .is_some_and(|e| e.kind() == std::io::ErrorKind::StorageFull)
                        })
                    });
                    let _ = tx.send(Event::Downloaded(
                        token,
                        result.map_err(|e| format!("{e:#}")),
                        storage_full,
                    ));
                });
            }
        }
        if self.dirty {
            self.refresh_pending();
        }
        if !self.listing
            && self.last.elapsed() > Duration::from_secs(3)
            && self.listed_revision != cache::revision()
        {
            self.listing = true;
            self.last = Instant::now();
            let tx = self.tx.clone();
            let revision = cache::revision();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Listed(revision, cache::global().entries()));
            });
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whole_collection_is_queued_once_without_the_old_two_thousand_track_cap() {
        let mut manager = Manager::new(true);
        let tracks = (0..3005)
            .map(|i| crate::model::tests::track(&format!("song-{i}"), "Artist", 180))
            .collect::<Vec<_>>();
        manager.download(tracks.clone());
        manager.download(tracks);
        assert_eq!(manager.view.downloading.len(), 3005);
        assert!(manager.busy() && manager.view.busy);
        manager.cancel();
        assert!(manager.view.downloading.is_empty());
        assert!(!manager.busy() && !manager.view.busy);
    }
    #[test]
    fn storage_full_stops_the_remaining_queue_instead_of_retrying_every_track() {
        let mut manager = Manager::new(false);
        manager.listing = true;
        let token = Arc::new(AtomicU64::new(1));
        manager.active = Some(("active".into(), token.clone()));
        manager.download(vec![crate::model::tests::track("pending", "Artist", 180)]);
        manager
            .tx
            .send(Event::Downloaded(
                token,
                Err("synthetic storage full".into()),
                true,
            ))
            .unwrap();
        manager.tick(&Services::new(String::new(), String::new(), String::new()));
        assert!(!manager.busy());
        assert!(manager.view.downloading.is_empty());
        assert_eq!(manager.errors.len(), 1);
    }
    #[test]
    fn cancelled_job_cannot_finish_a_new_download_of_the_same_track() {
        let mut manager = Manager::new(false);
        manager.last = Instant::now();
        let old = Arc::new(AtomicU64::new(1));
        manager.active = Some(("same-track".into(), old.clone()));
        manager.cancel();
        assert_eq!(old.load(Ordering::Relaxed), 2);
        let current = Arc::new(AtomicU64::new(1));
        manager.active = Some(("same-track".into(), current.clone()));
        manager
            .tx
            .send(Event::Downloaded(
                old,
                Err("old cancelled failure".into()),
                false,
            ))
            .unwrap();
        manager.tick(&Services::new(String::new(), String::new(), String::new()));
        assert!(Arc::ptr_eq(&manager.active.as_ref().unwrap().1, &current));
        assert!(manager.errors.is_empty());
        assert!(manager.view.busy);
    }
}
