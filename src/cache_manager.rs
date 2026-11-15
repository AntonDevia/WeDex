use crate::{
    cache::{self, View},
    model::Track,
    providers::Services,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

enum Event {
    Listed(u64, Vec<cache::Entry>),
    Downloaded(Arc<AtomicU64>, Result<(), String>),
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
        self.dirty = true;
        for track in tracks.into_iter().take(2000) {
            let key = track.key();
            if !self.queue.iter().any(|t| t.key() == key)
                && self.active.as_ref().is_none_or(|(k, _)| k != &key)
            {
                self.queue.push_back(track);
            }
        }
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
                    self.listed_revision = revision;
                    self.listing = false;
                    self.view.bytes = entries.iter().map(|e| e.size).sum();
                    self.view.entries = Arc::new(entries);
                }
                Event::Downloaded(token, result) => {
                    self.dirty = true;
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|(_, t)| Arc::ptr_eq(t, &token))
                    {
                        self.active = None;
                        if let Err(e) = result {
                            if e.contains("Лимит хранилища") {
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
        if self.active.is_none() {
            if let Some(track) = self.queue.pop_front() {
                self.dirty = true;
                let tx = self.tx.clone();
                let service = services.clone();
                let key = track.key();
                let token = Arc::new(AtomicU64::new(1));
                self.active = Some((key.clone(), token.clone()));
                std::thread::spawn(move || {
                    let result = crate::audio::download_offline(&track, &service, token.clone())
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Event::Downloaded(token, result));
                });
            }
        }
        if self.dirty {
            self.dirty = false;
            self.view.downloading = Arc::new(
                self.active
                    .iter()
                    .map(|(k, _)| k.clone())
                    .chain(self.queue.iter().map(Track::key))
                    .collect(),
            );
            self.view.busy = !self.view.downloading.is_empty();
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
            .send(Event::Downloaded(old, Err("old cancelled failure".into())))
            .unwrap();
        manager.tick(&Services::new(String::new(), String::new(), String::new()));
        assert!(Arc::ptr_eq(&manager.active.as_ref().unwrap().1, &current));
        assert!(manager.errors.is_empty());
        assert!(manager.view.busy);
    }
}
