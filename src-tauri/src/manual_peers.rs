// Millennium Clipboard — manual peers (Fase 8)
//
// Peers entered by hand (IP + port). Useful on networks where mDNS is
// blocked (AP isolation, VLAN segmentation in offices). The entries are
// persisted; a periodic poller in `discovery.rs` checks reachability.
//
// I/O (atomic write + backup-on-corrupt) is delegated to JsonStore.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::json_store::JsonStore;

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
    store: JsonStore<ManualPeers>,
}

impl ManualPeerStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store: JsonStore<ManualPeers> = JsonStore::load(data_dir, "manual-peers", "json")?;
        let n = store.read(|p| p.peers.len());
        crate::runtime_log::info(format!("[manual] loaded {} manual peer(s)", n));
        Ok(Self { store })
    }

    pub fn snapshot(&self) -> Vec<ManualPeer> {
        self.store.read(|p| p.peers.values().cloned().collect())
    }

    pub fn contains(&self, fingerprint: &str) -> bool {
        self.store.read(|p| p.peers.contains_key(fingerprint))
    }

    pub fn add(&self, peer: ManualPeer) -> Result<()> {
        self.store.update(|p| {
            p.peers.insert(peer.fingerprint.clone(), peer);
        })
    }

    pub fn remove(&self, fingerprint: &str) -> Result<()> {
        self.store.update(|p| {
            p.peers.remove(fingerprint);
        })
    }
}
