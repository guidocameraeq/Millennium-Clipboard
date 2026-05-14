// Millennium Clipboard — user settings (Fase 7)
//
// Persistent user preferences distinct from peer favorites:
//   - download_dir: where incoming files land
//   - auto_accept_favorites: skip the accept prompt for trusted peers

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    #[serde(default)]
    pub register_send_to: bool,
}

fn default_auto_accept() -> bool { false }
fn default_notifications_enabled() -> bool { true }
fn default_close_to_tray() -> bool { true }

pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load_or_default(data_dir: &Path, default_download_dir: PathBuf) -> Result<Self> {
        eprintln!("[settings::load] enter, data_dir={}", data_dir.display());
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("settings-{}.json", s),
            _ => "settings.json".to_string(),
        };
        let path = data_dir.join(&filename);
        eprintln!("[settings::load] target file = {}", path.display());

        let exists = path.exists();
        eprintln!("[settings::load] exists = {}", exists);

        let inner = if exists {
            eprintln!("[settings::load] reading file...");
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            eprintln!("[settings::load] read {} bytes, parsing...", raw.len());
            serde_json::from_str::<Settings>(&raw).unwrap_or_else(|e| {
                eprintln!("[settings::load] parse failed ({}), using defaults", e);
                Settings {
                    download_dir: default_download_dir.clone(),
                    auto_accept_favorites: false,
                    notifications_enabled: true,
                    start_with_windows: false,
                    close_to_tray: true,
                    register_send_to: false,
                }
            })
        } else {
            eprintln!("[settings::load] file missing, using defaults");
            Settings {
                download_dir: default_download_dir,
                auto_accept_favorites: false,
                notifications_enabled: true,
                start_with_windows: false,
                close_to_tray: true,
                register_send_to: false,
            }
        };

        eprintln!("[settings::load] building store...");
        let store = Self { path, inner: Mutex::new(inner) };
        let s = store.inner.lock().unwrap();
        eprintln!(
            "[settings] download_dir={} auto_accept_favorites={}",
            s.download_dir.display(),
            s.auto_accept_favorites,
        );
        drop(s);
        Ok(store)
    }

    pub fn snapshot(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    pub fn set_download_dir(&self, dir: PathBuf) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.download_dir = dir;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
        };
        self.persist(payload)
    }

    pub fn set_auto_accept_favorites(&self, value: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.auto_accept_favorites = value;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
        };
        self.persist(payload)
    }

    pub fn set_notifications_enabled(&self, value: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.notifications_enabled = value;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
        };
        self.persist(payload)
    }

    pub fn set_start_with_windows(&self, value: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.start_with_windows = value;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
        };
        self.persist(payload)
    }

    pub fn set_close_to_tray(&self, value: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.close_to_tray = value;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
        };
        self.persist(payload)
    }

    pub fn set_register_send_to(&self, value: bool) -> Result<()> {
        let payload = {
            let mut s = self.inner.lock().unwrap();
            s.register_send_to = value;
            serde_json::to_string_pretty(&*s).context("serialize settings")?
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
