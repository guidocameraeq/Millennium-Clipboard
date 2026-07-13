// Millennium Clipboard — preferences (Fase 6)
//
// Persists user-level state that should survive between runs.
// Favorites store the *whole* peer card (alias, hex, icon, last seen
// IP/port) so the UI can show a favorite even when it's currently
// offline and not in the mDNS cache.
//
// I/O (atomic write + backup-on-corrupt) is delegated to JsonStore; this
// module only owns the shape of the data and the public API.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::json_store::JsonStore;

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
    store: JsonStore<Preferences>,
}

impl PreferencesStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store: JsonStore<Preferences> = JsonStore::load(data_dir, "prefs", "json")?;
        let n = store.read(|p| p.favorites.len());
        crate::runtime_log::info(format!("[prefs] loaded {} favorite(s)", n));
        Ok(Self { store })
    }

    pub fn is_favorite(&self, fingerprint: &str) -> bool {
        self.store.read(|p| p.favorites.contains_key(fingerprint))
    }

    pub fn add_favorite(&self, peer: FavoritePeer) -> Result<()> {
        self.store.update(|p| {
            p.favorites.insert(peer.fingerprint.clone(), peer);
        })
    }

    pub fn remove_favorite(&self, fingerprint: &str) -> Result<()> {
        self.store.update(|p| {
            p.favorites.remove(fingerprint);
        })
    }

    pub fn favorites_snapshot(&self) -> Vec<FavoritePeer> {
        self.store.read(|p| p.favorites.values().cloned().collect())
    }
}
