//! Completed original audio bytes. Partial responses never become offline tracks.
use crate::model::{Provider, Track};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};

static AUTO: AtomicBool = AtomicBool::new(true);
static OFFLINE: AtomicBool = AtomicBool::new(false);
static LIMIT: AtomicU64 = AtomicU64::new(2048 * 1024 * 1024);
static WRITER: Mutex<()> = Mutex::new(());
static REVISION: AtomicU64 = AtomicU64::new(0);
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}
pub const MAX_TRACK: usize = 128 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub track: Track,
    pub source: Provider,
    pub size: u64,
    pub digest: String,
    pub saved: u64,
    pub accessed: u64,
    pub pinned: bool,
}
#[derive(Clone, Default)]
pub struct View {
    pub entries: std::sync::Arc<Vec<Entry>>,
    pub bytes: u64,
    pub downloading: std::sync::Arc<Vec<String>>,
    pub busy: bool,
}
#[derive(Clone)]
pub struct Disk {
    pub(super) root: PathBuf,
    pub(super) limit: u64,
}
pub fn configure(auto: bool, megabytes: u32) {
    AUTO.store(auto, Ordering::Relaxed);
    LIMIT.store(
        u64::from(megabytes.clamp(256, 16384)) * 1024 * 1024,
        Ordering::Relaxed,
    );
}
pub fn automatic() -> bool {
    AUTO.load(Ordering::Relaxed)
}
pub fn set_offline(value: bool) {
    OFFLINE.store(value, Ordering::Relaxed);
}
pub fn offline() -> bool {
    OFFLINE.load(Ordering::Relaxed)
}
fn key(track: &Track) -> String {
    format!("{:x}", md5::compute(track.key()))
}
fn now() -> u64 {
    wedex_cloud::now_ms()
}
pub fn global() -> Disk {
    Disk {
        root: crate::store::data_dir().join("audio-cache-v1"),
        limit: LIMIT.load(Ordering::Relaxed),
    }
}
fn audio(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && (bytes.starts_with(b"ID3")
            || bytes.starts_with(b"RIFF")
            || bytes.starts_with(b"OggS")
            || bytes.starts_with(b"fLaC")
            || bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0
            || &bytes[4..8] == b"ftyp")
}
fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("{}.part", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        std::fs::write(&temp, bytes)?;
        // Only app-owned, content-addressed cache files are replaced.
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
impl Disk {
    pub fn entries(&self) -> Vec<Entry> {
        let Ok(items) = std::fs::read_dir(&self.root) else {
            return vec![];
        };
        let mut entries: Vec<_> = items
            .filter_map(|p| {
                let p = p.ok()?.path();
                if p.extension()?.to_str()? != "json" || std::fs::metadata(&p).ok()?.len() > 32768 {
                    return None;
                }
                let e: Entry = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
                if p.file_stem()?.to_str()? != key(&e.track) || e.size > MAX_TRACK as u64 {
                    return None;
                }
                (std::fs::metadata(self.root.join(format!("{}.audio", key(&e.track))))
                    .ok()?
                    .len()
                    == e.size)
                    .then_some(e)
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.saved));
        entries
    }
    pub fn read(&self, track: &Track) -> Option<Vec<u8>> {
        let id = key(track);
        let metadata = self.root.join(format!("{id}.json"));
        if std::fs::metadata(&metadata).ok()?.len() > 32768 {
            return None;
        }
        let mut entry: Entry = serde_json::from_slice(&std::fs::read(&metadata).ok()?).ok()?;
        if entry.track.key() != track.key() || entry.size > MAX_TRACK as u64 {
            return None;
        }
        let path = self.root.join(format!("{id}.audio"));
        if std::fs::metadata(&path).ok()?.len() != entry.size {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        if format!("{:x}", md5::compute(&bytes)) != entry.digest || !audio(&bytes) {
            return None;
        }
        entry.accessed = now();
        if let Ok(_guard) = WRITER.try_lock() {
            if let Ok(data) = serde_json::to_vec(&entry) {
                let _ = replace(&metadata, &data);
            }
        }
        Some(bytes)
    }
    pub fn save(&self, track: &Track, source: Provider, bytes: &[u8], pinned: bool) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_TRACK && audio(bytes),
            "Загрузка не является полной аудиозаписью"
        );
        let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(&self.root)?;
        let id = key(track);
        let mut entries = self.entries();
        let pinned = pinned
            || entries
                .iter()
                .any(|e| e.track.key() == track.key() && e.pinned);
        entries.retain(|e| e.track.key() != track.key());
        let mut total = entries.iter().map(|e| e.size).sum::<u64>() + bytes.len() as u64;
        entries.sort_by_key(|e| e.accessed);
        ensure!(
            entries
                .iter()
                .filter(|e| e.pinned)
                .map(|e| e.size)
                .sum::<u64>()
                + bytes.len() as u64
                <= self.limit,
            "Лимит хранилища занят скачанными треками. Увеличьте его в настройках"
        );
        for old in entries.iter().filter(|e| !e.pinned) {
            if total <= self.limit {
                break;
            }
            self.remove_inner(&old.track)?;
            total = total.saturating_sub(old.size);
        }
        replace(&self.root.join(format!("{id}.audio")), bytes)
            .context("Сохранение аудио на устройстве")?;
        let entry = Entry {
            track: track.clone(),
            source,
            size: bytes.len() as u64,
            digest: format!("{:x}", md5::compute(bytes)),
            saved: now(),
            accessed: now(),
            pinned,
        };
        replace(
            &self.root.join(format!("{id}.json")),
            &serde_json::to_vec(&entry)?,
        )?;
        REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    pub fn pin(&self, track: &Track) -> Result<()> {
        let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.root.join(format!("{}.json", key(track)));
        let mut entry: Entry = serde_json::from_slice(&std::fs::read(&path)?)?;
        entry.pinned = true;
        replace(&path, &serde_json::to_vec(&entry)?)?;
        REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn remove_inner(&self, track: &Track) -> Result<()> {
        REVISION.fetch_add(1, Ordering::Relaxed);
        let id = key(track);
        for extension in ["json", "audio"] {
            let path = self.root.join(format!("{id}.{extension}"));
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        if let Some(source) = &track.cover {
            if !self
                .entries()
                .iter()
                .any(|e| e.track.cover.as_ref() == Some(source))
            {
                let path = self.cover_path(source);
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }
        Ok(())
    }
    pub fn remove(&self, track: &Track) -> Result<()> {
        let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        self.remove_inner(track)
    }
    pub fn clear_temporary(&self) -> Result<()> {
        let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        for entry in self.entries().into_iter().filter(|e| !e.pinned) {
            self.remove_inner(&entry.track)?;
        }
        Ok(())
    }
    fn cover_path(&self, source: &str) -> PathBuf {
        self.root.join(format!("{:x}.cover", md5::compute(source)))
    }
    pub fn cover(&self, source: &str) -> Option<Vec<u8>> {
        let path = self.cover_path(source);
        if std::fs::metadata(&path).ok()?.len() > 512 * 1024 {
            return None;
        }
        std::fs::read(path).ok()
    }
    pub fn save_cover(&self, track: &Track, http: &reqwest::blocking::Client) {
        use std::io::Read;
        let Some(source) = track
            .cover
            .as_deref()
            .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        else {
            return;
        };
        if self.cover(source).is_some() {
            return;
        }
        let result = (|| -> Result<Vec<u8>> {
            let response = http
                .get(source)
                .timeout(std::time::Duration::from_secs(5))
                .send()?
                .error_for_status()?;
            let mut bytes = Vec::new();
            response.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 4 * 1024 * 1024, "Обложка слишком большая");
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            let (w, h) = reader.into_dimensions()?;
            ensure!(w <= 4096 && h <= 4096, "Обложка слишком большая");
            let image = image::load_from_memory(&bytes)?
                .thumbnail(600, 600)
                .to_rgb8();
            let mut out = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
                .encode_image(&image)?;
            ensure!(out.len() <= 512 * 1024, "Обложка слишком большая");
            Ok(out)
        })();
        if let Ok(bytes) = result {
            let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
            if self.root.join(format!("{}.json", key(track))).exists() {
                let _ = replace(&self.cover_path(source), &bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_eviction_and_cleanup_preserve_pinned_downloads() {
        let path = std::env::temp_dir().join(format!("wedex-cache-test-{}", uuid::Uuid::new_v4()));
        let disk = Disk {
            root: path.clone(),
            limit: 220,
        };
        let a = crate::model::tests::track("Pinned", "Artist", 10);
        let b = crate::model::tests::track("Temporary", "Artist", 10);
        let c = crate::model::tests::track("New temporary", "Artist", 10);
        let mut bytes = vec![0; 100];
        bytes[..3].copy_from_slice(b"ID3");
        disk.save(&a, Provider::Hitmotop, &bytes, true).unwrap();
        disk.save(&b, Provider::Hitmotop, &bytes, false).unwrap();
        disk.save(&c, Provider::Hitmotop, &bytes, false).unwrap();
        assert!(disk.read(&a).is_some());
        assert!(disk.read(&b).is_none());
        assert!(disk.read(&c).is_some());
        disk.clear_temporary().unwrap();
        assert_eq!(disk.entries().len(), 1);
        let mut larger = bytes.clone();
        larger.resize(150, 0);
        assert!(disk.save(&b, Provider::Hitmotop, &larger, true).is_err());
        disk.remove(&a).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn completed_audio_survives_restart_and_corruption_is_not_played() {
        let path = std::env::temp_dir().join(format!("wedex-cache-test-{}", uuid::Uuid::new_v4()));
        let disk = Disk {
            root: path.clone(),
            limit: 1000,
        };
        let t = crate::model::tests::track("Song", "Artist", 10);
        let mut bytes = vec![0; 100];
        bytes[..3].copy_from_slice(b"ID3");
        disk.save(&t, Provider::Hitmotop, &bytes, true).unwrap();
        let restarted = Disk {
            root: path.clone(),
            limit: 1000,
        };
        assert_eq!(restarted.read(&t).unwrap(), bytes);
        assert!(restarted.entries()[0].pinned);
        std::fs::write(path.join(format!("{}.audio", key(&t))), vec![1; 100]).unwrap();
        assert!(restarted.read(&t).is_none());
        assert!(restarted
            .save(&t, Provider::Hitmotop, b"<html>bad response</html>", false)
            .is_err());
        restarted.remove(&t).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
}
