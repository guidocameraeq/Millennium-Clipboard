// Millennium Clipboard — alias overrides (v0.5.0 F3)
//
// Per-peer display name override. When set, the UI shows this string
// instead of whatever the remote advertised via /info or mDNS. Works
// across mDNS peers, manual peers and offline favorites — the
// fingerprint is the stable key.
//
// I/O (atomic write + backup-on-corrupt) is delegated to JsonStore.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::json_store::JsonStore;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Aliases {
    #[serde(default)]
    overrides: HashMap<String, String>,
}

pub struct AliasStore {
    store: JsonStore<Aliases>,
}

impl AliasStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store: JsonStore<Aliases> = JsonStore::load(data_dir, "aliases", "json")?;
        let n = store.read(|a| a.overrides.len());
        crate::runtime_log::info(format!("[aliases] loaded {} override(s)", n));
        Ok(Self { store })
    }

    pub fn get(&self, fingerprint: &str) -> Option<String> {
        self.store.read(|a| a.overrides.get(fingerprint).cloned())
    }

    pub fn set(&self, fingerprint: String, alias: String) -> Result<()> {
        self.store.update(|a| {
            a.overrides.insert(fingerprint, alias);
        })
    }

    pub fn clear(&self, fingerprint: &str) -> Result<()> {
        self.store.update(|a| {
            a.overrides.remove(fingerprint);
        })
    }
}
