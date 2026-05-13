// Millennium Clipboard — manual peers (Fase 8)
//
// Peers entered by hand (IP + port). Useful on networks where mDNS is
// blocked (AP isolation, VLAN segmentation in offices). The entries are
// persisted; a periodic poller in `discovery.rs` checks reachability.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualPeer {
    pub fingerprint: String,
    pub alias: String,
    pub hex_id: String,
    pub icon_type: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManualPeers {
    #[serde(default)]
    peers: HashMap<String, ManualPeer>,
}

pub struct ManualPeerStore {
    path: PathBuf,
    inner: Mutex<ManualPeers>,
}

impl ManualPeerStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("manual-peers-{}.json", s),
            _ => "manual-peers.json".to_string(),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str::<ManualPeers>(&raw).unwrap_or_default()
        } else {
            ManualPeers::default()
        };

        let n = inner.peers.len();
        let store = Self { path, inner: Mutex::new(inner) };
        eprintln!("[manual] loaded {} manual peer(s) from {}", n, store.path.display());
        Ok(store)
    }

    pub fn snapshot(&self) -> Vec<ManualPeer> {
        self.inner.lock().unwrap().peers.values().cloned().collect()
    }

    pub fn contains(&self, fingerprint: &str) -> bool {
        self.inner.lock().unwrap().peers.contains_key(fingerprint)
    }

    pub fn add(&self, peer: ManualPeer) -> Result<()> {
        let payload = {
            let mut p = self.inner.lock().unwrap();
            p.peers.insert(peer.fingerprint.clone(), peer);
            serde_json::to_string_pretty(&*p).context("serialize manual peers")?
        };
        self.persist(payload)
    }

    pub fn remove(&self, fingerprint: &str) -> Result<()> {
        let payload = {
            let mut p = self.inner.lock().unwrap();
            p.peers.remove(fingerprint);
            serde_json::to_string_pretty(&*p).context("serialize manual peers")?
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
