// Millennium Clipboard — clipboard sync (v0.6.0)
//
// Per-peer "clipboard partner" toggle. When BOTH ends enable sync with
// each other, the locally copied text propagates to the peer's
// clipboard, and vice versa. Mutual consent is the safety guarantee:
// the receiver rejects /clipboard payloads from peers it hasn't opted
// in to.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ClipSync {
    #[serde(default)]
    enabled: HashSet<String>,
}

pub struct ClipboardSyncStore {
    path: PathBuf,
    inner: Mutex<ClipSync>,
    /// Hash of the most recent text we either accepted from a peer or
    /// sent out. Used to break the broadcast → receive → re-broadcast
    /// loop: if the local clipboard equals this, skip the next round.
    last_synced_hash: Mutex<Option<String>>,
}

impl ClipboardSyncStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("clipboard-sync-{}.json", s),
            _ => "clipboard-sync.json".to_string(),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str::<ClipSync>(&raw).unwrap_or_default()
        } else {
            ClipSync::default()
        };

        let n = inner.enabled.len();
        let store = Self {
            path,
            inner: Mutex::new(inner),
            last_synced_hash: Mutex::new(None),
        };
        eprintln!("[clipboard] sync enabled for {} peer(s)", n);
        Ok(store)
    }

    pub fn is_enabled(&self, fingerprint: &str) -> bool {
        self.inner.lock().unwrap().enabled.contains(fingerprint)
    }

    pub fn set(&self, fingerprint: String, enabled: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            if enabled {
                s.enabled.insert(fingerprint);
            } else {
                s.enabled.remove(&fingerprint);
            }
            serde_json::to_string_pretty(&*s).context("serialize clipboard sync")?
        };
        self.persist(payload)
    }

    pub fn enabled_snapshot(&self) -> Vec<String> {
        self.inner.lock().unwrap().enabled.iter().cloned().collect()
    }

    pub fn note_synced(&self, hash: String) {
        *self.last_synced_hash.lock().unwrap() = Some(hash);
    }

    pub fn is_recent(&self, hash: &str) -> bool {
        self.last_synced_hash
            .lock()
            .unwrap()
            .as_deref()
            .map(|h| h == hash)
            .unwrap_or(false)
    }

    fn persist(&self, payload: String) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&self.path, payload)
            .with_context(|| format!("write {}", self.path.display()))?;
        Ok(())
    }
}

pub fn hash_text(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}
