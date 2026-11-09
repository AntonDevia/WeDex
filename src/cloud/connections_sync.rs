use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wedex_cloud::vault::{self, Entry};
#[derive(Default, Serialize, Deserialize)]
pub struct Sync {
    entries: BTreeMap<String, Entry>,
    pending: BTreeSet<String>,
    #[serde(skip)]
    pub values: BTreeMap<String, String>,
    #[serde(skip)]
    pub changes: Vec<(String, String)>,
    #[serde(skip)]
    pub ready: bool,
}

impl Sync {
    pub fn edited(
        &mut self,
        key: &str,
        writer: &str,
        name: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        let clock = self.entries.values().map(|e| e.clock).max().unwrap_or(0) + 1;
        self.entries.insert(
            name.into(),
            Entry {
                name: name.into(),
                clock,
                writer: writer.into(),
                encrypted: vault::seal(key, name, value)?,
            },
        );
        self.pending.insert(name.into());
        Ok(())
    }
    pub fn merge(&mut self, key: &str, writer: &str, remote: Vec<Entry>) -> anyhow::Result<()> {
        for entry in remote {
            anyhow::ensure!(entry.valid(), "Некорректная запись синхронизации");
            let order = (entry.clock, entry.writer.as_str());
            let own = self.entries.get(&entry.name);
            if let Some(own) = own.filter(|_| self.pending.contains(&entry.name)) {
                if own == &entry {
                    self.pending.remove(&entry.name);
                } else if order > (own.clock, own.writer.as_str()) {
                    let value = vault::open(key, &own.name, &own.encrypted)?;
                    let next = Entry {
                        name: entry.name.clone(),
                        clock: entry.clock + 1,
                        writer: writer.into(),
                        encrypted: vault::seal(key, &entry.name, &value)?,
                    };
                    self.entries.insert(entry.name.clone(), next);
                }
                continue;
            }
            if own.is_some_and(|own| (own.clock, own.writer.as_str()) > order) {
                continue;
            }
            let value = vault::open(key, &entry.name, &entry.encrypted)?;
            if self.values.get(&entry.name) != Some(&value) {
                self.changes.push((entry.name.clone(), value));
            }
            self.entries.insert(entry.name.clone(), entry);
        }
        for (name, value) in self.values.clone() {
            if !value.is_empty() && !self.entries.contains_key(&name) {
                self.edited(key, writer, &name, &value)?;
            }
        }
        self.ready = true;
        Ok(())
    }
    pub fn pending(&self) -> Vec<Entry> {
        self.pending
            .iter()
            .filter_map(|n| self.entries.get(n).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_import_offline_changes_and_disconnect_tombstones_merge() {
        let key = vault::new_key().unwrap();
        let a = uuid::Uuid::new_v4().to_string();
        let b = uuid::Uuid::new_v4().to_string();
        let mut pc = Sync::default();
        pc.values.insert("lastfm".into(), "fixture-key".into());
        pc.merge(&key, &a, vec![]).unwrap();
        let initial = pc.pending();
        let mut phone = Sync::default();
        phone.merge(&key, &b, initial.clone()).unwrap();
        assert_eq!(phone.changes, vec![("lastfm".into(), "fixture-key".into())]);
        pc.merge(&key, &a, initial).unwrap();
        assert!(pc.pending().is_empty());
        phone
            .edited(&key, &b, "hitmo", "https://rus.hitmos.fm/")
            .unwrap();
        pc.merge(&key, &a, phone.pending()).unwrap();
        assert!(pc
            .changes
            .contains(&("hitmo".into(), "https://rus.hitmos.fm/".into())));
        pc.edited(&key, &a, "lastfm", "").unwrap();
        phone.merge(&key, &b, pc.pending()).unwrap();
        assert!(phone.changes.contains(&("lastfm".into(), String::new())));
        let bytes = serde_json::to_string(&pc).unwrap();
        assert!(!bytes.contains("fixture-key") && !bytes.contains("rus.hitmos.fm"));
    }
}
