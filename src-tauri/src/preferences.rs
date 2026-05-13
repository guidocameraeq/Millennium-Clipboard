// Millennium Clipboard — preferences (Fase 6)
//
// Persists user-level state that should survive between runs.
// Favorites store the *whole* peer card (alias, hex, icon, last seen
// IP/port) so the UI can show a favorite even when it's currently
// offline and not in the mDNS cache.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoritePeer {
    pub fingerprint: String,
    pub alias: String,
    pub hex_id: String,
    pub icon_type: String,
    pub last_ip: String,
    pub last_port: u16,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Preferences {
    #[serde(default)]
    favorites: HashMap<String, FavoritePeer>,
}

pub struct PreferencesStore {
    path: PathBuf,
    inner: Mutex<Preferences>,
}

impl PreferencesStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("prefs-{}.json", s),
            _ => "prefs.json".to_string(),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str::<Preferences>(&raw).unwrap_or_default()
        } else {
            Preferences::default()
        };

        let store = Self { path, inner: Mutex::new(inner) };
        println!(
            "[prefs] loaded {} favorite(s) from {}",
            store.inner.lock().unwrap().favorites.len(),
            store.path.display()
        );
        Ok(store)
    }

    pub fn is_favorite(&self, fingerprint: &str) -> bool {
        self.inner.lock().unwrap().favorites.contains_key(fingerprint)
    }

    pub fn add_favorite(&self, peer: FavoritePeer) -> Result<()> {
        let snapshot = {
            let mut p = self.inner.lock().unwrap();
            p.favorites.insert(peer.fingerprint.clone(), peer);
            serde_json::to_string_pretty(&*p).context("serialize prefs")?
        };
        self.persist(snapshot)
    }

    pub fn remove_favorite(&self, fingerprint: &str) -> Result<()> {
        let snapshot = {
            let mut p = self.inner.lock().unwrap();
            p.favorites.remove(fingerprint);
            serde_json::to_string_pretty(&*p).context("serialize prefs")?
        };
        self.persist(snapshot)
    }

    pub fn favorites_snapshot(&self) -> Vec<FavoritePeer> {
        self.inner.lock().unwrap().favorites.values().cloned().collect()
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
