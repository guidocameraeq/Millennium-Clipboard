// Millennium Clipboard — per-peer icon overrides (v0.8.6)
//
// Stores a user-chosen icon for each peer fingerprint. When set, the
// frontend renders this icon in the card instead of whatever the
// remote advertised in `icon_type` (desktop / phone / etc).
//
// Mirrors AliasStore's shape — same persistence rules, same MILLENNIUM_INSTANCE
// behavior for dev double-launch. I/O is delegated to JsonStore.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::json_store::JsonStore;

#[derive(Debug, Default, Serialize, Deserialize)]
struct IconOverrides {
    #[serde(default)]
    overrides: HashMap<String, String>,
}

pub struct IconOverrideStore {
    store: JsonStore<IconOverrides>,
}

impl IconOverrideStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store: JsonStore<IconOverrides> = JsonStore::load(data_dir, "icon-overrides", "json")?;
        let n = store.read(|a| a.overrides.len());
        crate::runtime_log::info(format!("[icons] loaded {} icon override(s)", n));
        Ok(Self { store })
    }

    pub fn get(&self, fingerprint: &str) -> Option<String> {
        self.store.read(|a| a.overrides.get(fingerprint).cloned())
    }

    pub fn set(&self, fingerprint: String, icon: String) -> Result<()> {
        self.store.update(|a| {
            a.overrides.insert(fingerprint, icon);
        })
    }

    pub fn clear(&self, fingerprint: &str) -> Result<()> {
        self.store.update(|a| {
            a.overrides.remove(fingerprint);
        })
    }
}
