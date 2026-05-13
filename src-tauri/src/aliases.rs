// Millennium Clipboard — alias overrides (v0.5.0 F3)
//
// Per-peer display name override. When set, the UI shows this string
// instead of whatever the remote advertised via /info or mDNS. Works
// across mDNS peers, manual peers and offline favorites — the
// fingerprint is the stable key.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Aliases {
    #[serde(default)]
    overrides: HashMap<String, String>,
}

pub struct AliasStore {
    path: PathBuf,
    inner: Mutex<Aliases>,
}

impl AliasStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("aliases-{}.json", s),
            _ => "aliases.json".to_string(),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str::<Aliases>(&raw).unwrap_or_default()
        } else {
            Aliases::default()
        };

        let n = inner.overrides.len();
        let store = Self { path, inner: Mutex::new(inner) };
        eprintln!("[aliases] loaded {} override(s)", n);
        Ok(store)
    }

    pub fn get(&self, fingerprint: &str) -> Option<String> {
        self.inner.lock().unwrap().overrides.get(fingerprint).cloned()
    }

    pub fn set(&self, fingerprint: String, alias: String) -> Result<()> {
        let payload = {
            let mut a = self.inner.lock().unwrap();
            a.overrides.insert(fingerprint, alias);
            serde_json::to_string_pretty(&*a).context("serialize aliases")?
        };
        self.persist(payload)
    }

    pub fn clear(&self, fingerprint: &str) -> Result<()> {
        let payload = {
            let mut a = self.inner.lock().unwrap();
            a.overrides.remove(fingerprint);
            serde_json::to_string_pretty(&*a).context("serialize aliases")?
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
