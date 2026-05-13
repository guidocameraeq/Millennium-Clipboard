// Millennium Clipboard — discovery (Fase 3 + Fase 4)
//
// mDNS service registration and browsing. Identity now comes from
// `identity.rs` (cert fingerprint), no longer a per-run UUID. TXT
// records advertise the fingerprint so peers can verify the cert they
// later see at /info matches what the discovery announced.

use crate::identity::Identity;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

pub const SERVICE_TYPE: &str = "_millennium._tcp.local.";
pub const SERVICE_PORT: u16 = 53319;

// ---------------------------------------------------------------------------
// Wire types — what reaches the frontend.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePeer {
    pub id: String, // fingerprint
    pub name: String,
    pub hex_id: String,
    pub ip: String,
    pub port: u16,
    pub status: &'static str,
    pub favorite: bool,
    pub icon_type: String,
}

#[derive(Debug, Clone)]
pub struct PeerRecord {
    pub id: String,
    pub name: String,
    pub hex_id: String,
    pub ip: String,
    pub port: u16,
    pub icon_type: String,
    #[allow(dead_code)]
    pub last_seen: Instant,
}

impl PeerRecord {
    pub fn to_wire(&self, favorite: bool) -> WirePeer {
        WirePeer {
            id: self.id.clone(),
            name: self.name.clone(),
            hex_id: self.hex_id.clone(),
            ip: self.ip.clone(),
            port: self.port,
            status: "online",
            favorite,
            icon_type: self.icon_type.clone(),
        }
    }
}

pub type PeerMap = Arc<Mutex<HashMap<String, PeerRecord>>>;
pub type FullnameMap = Arc<Mutex<HashMap<String, String>>>;

pub struct DiscoveryState {
    pub peers: PeerMap,
    pub fullnames: FullnameMap,
    #[allow(dead_code)]
    pub daemon: ServiceDaemon,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn start(app: AppHandle, identity: &Identity) -> Result<DiscoveryState, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let fullnames: FullnameMap = Arc::new(Mutex::new(HashMap::new()));

    register_self(&daemon, identity)?;

    let receiver = daemon.browse(SERVICE_TYPE)?;

    let peers_for_task = peers.clone();
    let fullnames_for_task = fullnames.clone();
    let my_fingerprint = identity.fingerprint.clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv_async().await {
                Ok(event) => {
                    handle_event(
                        event,
                        &my_fingerprint,
                        &peers_for_task,
                        &fullnames_for_task,
                        &app_handle,
                    );
                }
                Err(e) => {
                    eprintln!("[mdns] channel closed: {}", e);
                    break;
                }
            }
        }
    });

    Ok(DiscoveryState { peers, fullnames, daemon })
}

pub fn rebrowse(state: &DiscoveryState) -> Result<(), mdns_sd::Error> {
    state.daemon.browse(SERVICE_TYPE).map(|_| ())
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn detect_icon_type() -> &'static str {
    if cfg!(target_os = "android") || cfg!(target_os = "ios") {
        "phone"
    } else {
        "desktop"
    }
}

fn register_self(daemon: &ServiceDaemon, id: &Identity) -> Result<(), mdns_sd::Error> {
    let mut props = HashMap::new();
    props.insert("id".to_string(), id.fingerprint.clone());
    props.insert("alias".to_string(), id.alias.clone());
    props.insert("hex".to_string(), id.hex_id.clone());
    props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    props.insert("icon".to_string(), detect_icon_type().to_string());

    let instance_name = format!("millennium-{}", &id.fingerprint[..8]);
    let host = if id.alias.is_empty() {
        "host".to_string()
    } else {
        id.alias.to_lowercase()
    };

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &format!("{}.local.", host),
        id.local_ip.as_str(),
        SERVICE_PORT,
        Some(props),
    )?;
    daemon.register(service)?;
    println!(
        "[mdns] registered {} on {}:{} (fp={})",
        instance_name,
        id.local_ip,
        SERVICE_PORT,
        &id.fingerprint[..16]
    );
    Ok(())
}

fn handle_event(
    event: ServiceEvent,
    my_fingerprint: &str,
    peers: &PeerMap,
    fullnames: &FullnameMap,
    app: &AppHandle,
) {
    match event {
        ServiceEvent::ServiceResolved(info) => {
            let txt = info.get_properties();
            let id = txt
                .get_property_val_str("id")
                .map(|s| s.to_string())
                .unwrap_or_default();

            if id.is_empty() || id == my_fingerprint {
                return;
            }

            let alias = txt.get_property_val_str("alias").unwrap_or("?").to_string();
            let hex_id = txt
                .get_property_val_str("hex")
                .unwrap_or("0x??:??:??")
                .to_string();
            let icon_type = txt
                .get_property_val_str("icon")
                .unwrap_or("desktop")
                .to_string();
            let ip = info
                .get_addresses()
                .iter()
                .next()
                .map(|a| a.to_string())
                .unwrap_or_default();

            let record = PeerRecord {
                id: id.clone(),
                name: alias,
                hex_id,
                ip,
                port: info.get_port(),
                icon_type,
                last_seen: Instant::now(),
            };
            let fullname = info.get_fullname().to_string();

            {
                let mut p = peers.lock().unwrap();
                p.insert(id.clone(), record);
            }
            {
                let mut f = fullnames.lock().unwrap();
                f.insert(fullname, id);
            }
            emit_peers_changed(app, peers);
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            let removed_id = {
                let mut f = fullnames.lock().unwrap();
                f.remove(&fullname)
            };
            if let Some(id) = removed_id {
                peers.lock().unwrap().remove(&id);
                emit_peers_changed(app, peers);
            }
        }
        _ => {}
    }
}

fn emit_peers_changed(app: &AppHandle, peers: &PeerMap) {
    let snapshot: Vec<WirePeer> = peers
        .lock()
        .unwrap()
        .values()
        .map(|r| r.to_wire(false))
        .collect();
    let _ = app.emit("peers-changed", &snapshot);
}
