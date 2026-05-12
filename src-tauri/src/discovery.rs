// Millennium Clipboard — discovery (Fase 3)
//
// mDNS service registration and browsing. Peers are advertised on the
// LAN under `_millennium._tcp.local.` with TXT records carrying the
// alias, hex id, version and icon hint. A background task feeds a
// shared peer map and emits `peers-changed` to the frontend whenever
// the set changes.

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub const SERVICE_TYPE: &str = "_millennium._tcp.local.";
pub const SERVICE_PORT: u16 = 53319;

// ---------------------------------------------------------------------------
// Wire types — what reaches the frontend.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePeer {
    pub id: String,
    pub name: String,
    pub hex_id: String,
    pub ip: String,
    pub port: u16,
    pub status: &'static str,
    pub favorite: bool,
    pub icon_type: String,
}

// ---------------------------------------------------------------------------
// Local identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Identity {
    pub uuid: String,
    pub alias: String,
    pub hex_id: String,
    pub local_ip: String,
}

impl Identity {
    pub fn bootstrap() -> Self {
        let uuid = Uuid::new_v4().simple().to_string();
        let alias = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "UNKNOWN-HOST".into())
            .to_uppercase();
        let hex_id = format_hex(&uuid);
        let local_ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_default();
        Self { uuid, alias, hex_id, local_ip }
    }
}

fn format_hex(uuid_simple: &str) -> String {
    // uuid_simple is 32 hex chars. Take 6 chars and format as 0xAB:CD:EF.
    let chars: Vec<&str> = (0..6).step_by(2).map(|i| &uuid_simple[i..i + 2]).collect();
    format!("0x{}", chars.join(":").to_uppercase())
}

fn detect_icon_type() -> &'static str {
    // Conservative: everything that isn't a known phone/tablet target is desktop.
    if cfg!(target_os = "android") || cfg!(target_os = "ios") {
        "phone"
    } else {
        "desktop"
    }
}

// ---------------------------------------------------------------------------
// Peer record (server side)
// ---------------------------------------------------------------------------

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
pub type FullnameMap = Arc<Mutex<HashMap<String, String>>>; // fullname -> peer id

// ---------------------------------------------------------------------------
// Discovery state — held by the app
// ---------------------------------------------------------------------------

pub struct DiscoveryState {
    pub identity: Identity,
    pub peers: PeerMap,
    pub fullnames: FullnameMap,
    #[allow(dead_code)]
    pub daemon: ServiceDaemon, // kept alive for the process lifetime
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn start(app: AppHandle) -> Result<DiscoveryState, mdns_sd::Error> {
    let identity = Identity::bootstrap();
    let daemon = ServiceDaemon::new()?;
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let fullnames: FullnameMap = Arc::new(Mutex::new(HashMap::new()));

    // 1. Register our own service
    register_self(&daemon, &identity)?;

    // 2. Browse the network
    let receiver = daemon.browse(SERVICE_TYPE)?;

    // 3. Spawn the event loop
    let peers_for_task = peers.clone();
    let fullnames_for_task = fullnames.clone();
    let my_uuid = identity.uuid.clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv_async().await {
                Ok(event) => {
                    handle_event(
                        event,
                        &my_uuid,
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

    Ok(DiscoveryState { identity, peers, fullnames, daemon })
}

pub fn rebrowse(state: &DiscoveryState) -> Result<(), mdns_sd::Error> {
    // Trigger a fresh probe. mdns-sd will re-emit ServiceResolved for
    // peers it sees again.
    state.daemon.browse(SERVICE_TYPE).map(|_| ())
}

fn register_self(daemon: &ServiceDaemon, id: &Identity) -> Result<(), mdns_sd::Error> {
    let mut props = HashMap::new();
    props.insert("id".to_string(), id.uuid.clone());
    props.insert("alias".to_string(), id.alias.clone());
    props.insert("hex".to_string(), id.hex_id.clone());
    props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    props.insert("icon".to_string(), detect_icon_type().to_string());

    let instance_name = format!("millennium-{}", &id.uuid[..8]);
    let host = if id.alias.is_empty() { "host".to_string() } else { id.alias.to_lowercase() };

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &format!("{}.local.", host),
        id.local_ip.as_str(),
        SERVICE_PORT,
        Some(props),
    )?;
    daemon.register(service)?;
    println!("[mdns] registered {} on {}:{}", instance_name, id.local_ip, SERVICE_PORT);
    Ok(())
}

fn handle_event(
    event: ServiceEvent,
    my_uuid: &str,
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

            // Skip ourselves and any entry without an id
            if id.is_empty() || id == my_uuid {
                return;
            }

            let alias = txt.get_property_val_str("alias").unwrap_or("?").to_string();
            let hex_id = txt.get_property_val_str("hex").unwrap_or("0x??:??:??").to_string();
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
                let mut p = peers.lock().unwrap();
                p.remove(&id);
                drop(p);
                emit_peers_changed(app, peers);
            }
        }
        ServiceEvent::SearchStarted(_) | ServiceEvent::SearchStopped(_) => {}
        _ => {}
    }
}

fn emit_peers_changed(app: &AppHandle, peers: &PeerMap) {
    let snapshot: Vec<WirePeer> = peers
        .lock()
        .unwrap()
        .values()
        .map(|r| r.to_wire(false)) // favorite always false until Fase 6
        .collect();
    let _ = app.emit("peers-changed", &snapshot);
}
