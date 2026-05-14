// Millennium Clipboard — per-peer icon overrides (v0.8.6)
//
// Stores a user-chosen icon for each peer fingerprint. When set, the
// frontend renders this icon in the card instead of whatever the
// remote advertised in `icon_type` (desktop / phone / etc).
//
// Mirrors AliasStore's shape — same persistence rules, same MILLENNIUM_INSTANCE
// behavior for dev double-launch.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default, Serialize, Deserialize)]
struct IconOverrides {
    #[serde(default)]
    overrides: HashMap<String, String>,
}

pub struct IconOverrideStore {
    path: PathBuf,
    inner: Mutex<IconOverrides>,
}

impl IconOverrideStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("icon-overrides-{}.json", s),
            _ => "icon-overrides.json".to_string(),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str::<IconOverrides>(&raw).unwrap_or_default()
        } else {
            IconOverrides::default()
        };

        let n = inner.overrides.len();
        let store = Self {
            path,
            inner: Mutex::new(inner),
        };
        crate::runtime_log::info(format!("[icons] loaded {} icon override(s)", n));
        Ok(store)
    }

    pub fn get(&self, fingerprint: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .overrides
            .get(fingerprint)
            .cloned()
    }

    pub fn set(&self, fingerprint: String, icon: String) -> Result<()> {
        let payload = {
            let mut a = self.inner.lock().unwrap();
            a.overrides.insert(fingerprint, icon);
            serde_json::to_string_pretty(&*a).context("serialize icons")?
        };
        self.persist(payload)
    }

    pub fn clear(&self, fingerprint: &str) -> Result<()> {
        let payload = {
            let mut a = self.inner.lock().unwrap();
            a.overrides.remove(fingerprint);
            serde_json::to_string_pretty(&*a).context("serialize icons")?
        };
        self.persist(payload)
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
