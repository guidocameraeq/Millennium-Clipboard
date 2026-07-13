// Millennium Clipboard — user settings (Fase 7)
//
// Persistent user preferences distinct from peer favorites:
//   - download_dir: where incoming files land
//   - auto_accept_favorites: skip the accept prompt for trusted peers
//
// `Settings` has no sensible `Default` (download_dir is caller-computed),
// so this store uses `JsonStore::load_with_default` with an explicit
// fallback. I/O (atomic write + backup-on-corrupt) lives in JsonStore.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::json_store::JsonStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: PathBuf,
    #[serde(default = "default_auto_accept")]
    pub auto_accept_favorites: bool,
    #[serde(default = "default_notifications_enabled")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
}

fn default_auto_accept() -> bool {
    false
}
fn default_notifications_enabled() -> bool {
    true
}
fn default_close_to_tray() -> bool {
    true
}

pub struct SettingsStore {
    store: JsonStore<Settings>,
}

impl SettingsStore {
    pub fn load_or_default(data_dir: &Path, default_download_dir: PathBuf) -> Result<Self> {
        let default = Settings {
            download_dir: default_download_dir,
            auto_accept_favorites: default_auto_accept(),
            notifications_enabled: default_notifications_enabled(),
            start_with_windows: false,
            close_to_tray: default_close_to_tray(),
        };
        let store = JsonStore::load_with_default(data_dir, "settings", "json", default)?;
        // Keep the final diagnostic line the wild-bug reports rely on.
        let (dir, auto) = store.read(|s| (s.download_dir.clone(), s.auto_accept_favorites));
        eprintln!(
            "[settings] download_dir={} auto_accept_favorites={}",
            dir.display(),
            auto
        );
        Ok(Self { store })
    }

    pub fn snapshot(&self) -> Settings {
        self.store.read(|s| s.clone())
    }

    /// True if settings.json existed but couldn't be parsed, so the live
    /// state is the fallback default rather than the user's real prefs.
    /// The autostart heal skips the Run-key rewrite when this is true.
    pub fn loaded_from_corrupt(&self) -> bool {
        self.store.loaded_from_corrupt()
    }

    pub fn set_download_dir(&self, dir: PathBuf) -> Result<()> {
        self.store.update(|s| s.download_dir = dir)
    }

    pub fn set_auto_accept_favorites(&self, value: bool) -> Result<()> {
        self.store.update(|s| s.auto_accept_favorites = value)
    }

    pub fn set_notifications_enabled(&self, value: bool) -> Result<()> {
        self.store.update(|s| s.notifications_enabled = value)
    }

    pub fn set_start_with_windows(&self, value: bool) -> Result<()> {
        self.store.update(|s| s.start_with_windows = value)
    }

    pub fn set_close_to_tray(&self, value: bool) -> Result<()> {
        self.store.update(|s| s.close_to_tray = value)
    }
}
