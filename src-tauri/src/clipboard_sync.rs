// Millennium Clipboard — clipboard sync (v0.6.0)
//
// Per-peer "clipboard partner" toggle. When BOTH ends enable sync with
// each other, the locally copied text propagates to the peer's
// clipboard, and vice versa. Mutual consent is the safety guarantee:
// the receiver rejects /clipboard payloads from peers it hasn't opted
// in to.
//
// The persisted part (the set of enabled fingerprints) is delegated to
// JsonStore for atomic writes + backup-on-corrupt. The in-memory echo
// guard (`last_synced_hash`) is NOT persisted and stays a plain field.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use crate::json_store::JsonStore;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ClipSync {
    #[serde(default)]
    enabled: HashSet<String>,
}

pub struct ClipboardSyncStore {
    store: JsonStore<ClipSync>,
    /// Hash of the most recent text we either accepted from a peer or
    /// sent out. Used to break the broadcast → receive → re-broadcast
    /// loop: if the local clipboard equals this, skip the next round.
    /// Not persisted — purely runtime echo-suppression state.
    last_synced_hash: Mutex<Option<String>>,
}

impl ClipboardSyncStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store: JsonStore<ClipSync> = JsonStore::load(data_dir, "clipboard-sync", "json")?;
        let n = store.read(|s| s.enabled.len());
        crate::runtime_log::info(format!("[clipboard] sync enabled for {} peer(s)", n));
        Ok(Self {
            store,
            last_synced_hash: Mutex::new(None),
        })
    }

    pub fn is_enabled(&self, fingerprint: &str) -> bool {
        self.store.read(|s| s.enabled.contains(fingerprint))
    }

    pub fn set(&self, fingerprint: String, enabled: bool) -> Result<()> {
        self.store.update(|s| {
            if enabled {
                s.enabled.insert(fingerprint);
            } else {
                s.enabled.remove(&fingerprint);
            }
        })
    }

    pub fn enabled_snapshot(&self) -> Vec<String> {
        self.store.read(|s| s.enabled.iter().cloned().collect())
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
