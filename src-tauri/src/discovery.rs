// Millennium Clipboard — discovery (Fase 3 + Fase 4)
//
// mDNS service registration and browsing. Identity now comes from
// `identity.rs` (cert fingerprint), no longer a per-run UUID. TXT
// records advertise the fingerprint so peers can verify the cert they
// later see at /info matches what the discovery announced.

use crate::identity::Identity;
use crate::manual_peers::ManualPeerStore;
use crate::preferences::PreferencesStore;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

pub const SERVICE_TYPE: &str = "_millennium._tcp.local.";
pub const DEFAULT_PORT: u16 = 53319;

/// Resolve the local listening port. Defaults to 53319; can be
/// overridden by `MILLENNIUM_PORT` (useful for running multiple
/// instances on the same host during development).
pub fn local_port() -> u16 {
    std::env::var("MILLENNIUM_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

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
    pub manual: bool,
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
    pub fn to_wire(&self, favorite: bool, manual: bool) -> WirePeer {
        WirePeer {
            id: self.id.clone(),
            name: self.name.clone(),
            hex_id: self.hex_id.clone(),
            ip: self.ip.clone(),
            port: self.port,
            status: "online",
            favorite,
            icon_type: self.icon_type.clone(),
            manual,
        }
    }
}

pub type PeerMap = Arc<Mutex<HashMap<String, PeerRecord>>>;
pub type FullnameMap = Arc<Mutex<HashMap<String, String>>>;

pub struct DiscoveryState {
    pub peers: PeerMap,
    pub fullnames: FullnameMap,
    pub prefs: Arc<PreferencesStore>,
    pub manual: Arc<ManualPeerStore>,
    #[allow(dead_code)]
    pub daemon: ServiceDaemon,
}

/// Build the merged wire-list. Sources in priority order:
///   1. Peers currently visible on mDNS (status = online).
///   2. Manually-added peers we couldn't reach (status = offline, manual = true).
///   3. Favorite peers we haven't seen in this session (status = offline).
fn build_wire_list(
    peers: &PeerMap,
    prefs: &PreferencesStore,
    manual: &ManualPeerStore,
) -> Vec<WirePeer> {
    let online = peers.lock().unwrap();
    let mut result: Vec<WirePeer> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for record in online.values() {
        let is_fav = prefs.is_favorite(&record.id);
        let is_manual = manual.contains(&record.id);
        result.push(record.to_wire(is_fav, is_manual));
        seen.insert(record.id.clone());
    }

    // Manual peers that aren't reachable right now still belong on the list.
    for m in manual.snapshot() {
        if seen.contains(&m.fingerprint) {
            continue;
        }
        result.push(WirePeer {
            id: m.fingerprint.clone(),
            name: m.alias,
            hex_id: m.hex_id,
            ip: m.ip,
            port: m.port,
            status: "offline",
            favorite: prefs.is_favorite(&m.fingerprint),
            icon_type: m.icon_type,
            manual: true,
        });
        seen.insert(m.fingerprint);
    }

    for fav in prefs.favorites_snapshot() {
        if seen.contains(&fav.fingerprint) {
            continue;
        }
        result.push(WirePeer {
            id: fav.fingerprint,
            name: fav.alias,
            hex_id: fav.hex_id,
            ip: fav.last_ip,
            port: fav.last_port,
            status: "offline",
            favorite: true,
            icon_type: fav.icon_type,
            manual: false,
        });
    }
    result
}

impl DiscoveryState {
    pub fn peers_for_wire(&self) -> Vec<WirePeer> {
        build_wire_list(&self.peers, &self.prefs, &self.manual)
    }

    /// Build a FavoritePeer payload from a currently-known peer, falling
    /// back to manually-registered data so peers added by hand can still
    /// be marked as favorites.
    pub fn favorite_from_peer(&self, fingerprint: &str) -> Option<crate::preferences::FavoritePeer> {
        if let Some(r) = self.peers.lock().unwrap().get(fingerprint) {
            return Some(crate::preferences::FavoritePeer {
                fingerprint: r.id.clone(),
                alias: r.name.clone(),
                hex_id: r.hex_id.clone(),
                icon_type: r.icon_type.clone(),
                last_ip: r.ip.clone(),
                last_port: r.port,
            });
        }
        self.manual
            .snapshot()
            .into_iter()
            .find(|m| m.fingerprint == fingerprint)
            .map(|m| crate::preferences::FavoritePeer {
                fingerprint: m.fingerprint,
                alias: m.alias,
                hex_id: m.hex_id,
                icon_type: m.icon_type,
                last_ip: m.ip,
                last_port: m.port,
            })
    }

    pub fn emit_snapshot(&self, app: &AppHandle) {
        let snapshot = self.peers_for_wire();
        let _ = app.emit("peers-changed", &snapshot);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn start(
    app: AppHandle,
    identity: &Identity,
    port: u16,
    prefs: Arc<PreferencesStore>,
    manual: Arc<ManualPeerStore>,
) -> Result<DiscoveryState, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let fullnames: FullnameMap = Arc::new(Mutex::new(HashMap::new()));

    register_self(&daemon, identity, port)?;

    let receiver = daemon.browse(SERVICE_TYPE)?;

    let peers_for_task = peers.clone();
    let fullnames_for_task = fullnames.clone();
    let prefs_for_task = prefs.clone();
    let manual_for_task = manual.clone();
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
                        &prefs_for_task,
                        &manual_for_task,
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

    // Periodic re-browse + stale-peer reaper. mDNS-sd resolves a peer
    // once and then stays quiet — without our own poke, last_seen drifts
    // and looks like the peer left. Re-browsing every few seconds keeps
    // healthy peers' timestamps fresh; the reaper still drops peers that
    // stop answering for a generous window (covers ungraceful exits the
    // standard mDNS goodbye would normally signal).
    let peers_for_reaper = peers.clone();
    let fullnames_for_reaper = fullnames.clone();
    let prefs_for_reaper = prefs.clone();
    let manual_for_reaper = manual.clone();
    let app_for_reaper = app.clone();
    let daemon_for_reaper = daemon.clone();
    tauri::async_runtime::spawn(async move {
        const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(90);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(8));
        tick.tick().await; // skip the immediate first tick
        loop {
            tick.tick().await;

            // Nudge the network so live peers re-announce.
            let _ = daemon_for_reaper.browse(SERVICE_TYPE);

            let now = std::time::Instant::now();
            let mut removed_ids: Vec<String> = Vec::new();
            {
                let mut peers = peers_for_reaper.lock().unwrap();
                peers.retain(|id, record| {
                    let alive = now.duration_since(record.last_seen) < STALE_AFTER;
                    if !alive {
                        removed_ids.push(id.clone());
                    }
                    alive
                });
            }
            if !removed_ids.is_empty() {
                let mut fn_map = fullnames_for_reaper.lock().unwrap();
                fn_map.retain(|_, peer_id| !removed_ids.contains(peer_id));
                drop(fn_map);
                for id in &removed_ids {
                    eprintln!("[mdns] reaper dropped stale peer {}", &id[..16.min(id.len())]);
                }
                let snapshot =
                    build_wire_list(&peers_for_reaper, &prefs_for_reaper, &manual_for_reaper);
                let _ = app_for_reaper.emit("peers-changed", &snapshot);
            }
        }
    });

    // Manual-peer poller. Manual entries don't ride mDNS, so we ping
    // them on a schedule and treat a successful /info reply as
    // "online" by inserting/refreshing a record in the same cache.
    let peers_for_manual = peers.clone();
    let prefs_for_manual = prefs.clone();
    let manual_for_manual = manual.clone();
    let app_for_manual = app.clone();
    let my_fp_for_manual = identity.fingerprint.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(12));
        tick.tick().await; // skip immediate tick — give the rest of setup a beat
        loop {
            tick.tick().await;
            let entries = manual_for_manual.snapshot();
            if entries.is_empty() {
                continue;
            }
            let mut changed = false;
            for m in entries {
                if m.fingerprint == my_fp_for_manual {
                    continue;
                }
                match crate::http_client::fetch_info(&m.ip, m.port).await {
                    Ok(info) if info.fingerprint == m.fingerprint => {
                        let record = PeerRecord {
                            id: m.fingerprint.clone(),
                            name: info.alias,
                            hex_id: m.hex_id.clone(),
                            ip: m.ip.clone(),
                            port: m.port,
                            icon_type: m.icon_type.clone(),
                            last_seen: std::time::Instant::now(),
                        };
                        peers_for_manual
                            .lock()
                            .unwrap()
                            .insert(m.fingerprint.clone(), record);
                        changed = true;
                    }
                    Ok(_) => {
                        eprintln!(
                            "[manual] fingerprint drift at {}:{}, dropping from cache",
                            m.ip, m.port
                        );
                        if peers_for_manual.lock().unwrap().remove(&m.fingerprint).is_some() {
                            changed = true;
                        }
                    }
                    Err(_) => {
                        // unreachable; leave it as offline (presence handled by build_wire_list)
                    }
                }
            }
            if changed {
                let snapshot =
                    build_wire_list(&peers_for_manual, &prefs_for_manual, &manual_for_manual);
                let _ = app_for_manual.emit("peers-changed", &snapshot);
            }
        }
    });

    Ok(DiscoveryState { peers, fullnames, prefs, manual, daemon })
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

fn register_self(daemon: &ServiceDaemon, id: &Identity, port: u16) -> Result<(), mdns_sd::Error> {
    let mut props = HashMap::new();
    props.insert("id".to_string(), id.fingerprint.clone());
    props.insert("alias".to_string(), id.alias.clone());
    props.insert("hex".to_string(), id.hex_id.clone());
    props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    props.insert("icon".to_string(), detect_icon_type().to_string());

    // Add a per-port suffix so two instances on the same host (dev) can
    // both register without colliding on the mDNS instance name.
    let instance_name = format!("millennium-{}-{}", &id.fingerprint[..8], port);
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
        port,
        Some(props),
    )?;
    daemon.register(service)?;
    println!(
        "[mdns] registered {} on {}:{} (fp={})",
        instance_name,
        id.local_ip,
        port,
        &id.fingerprint[..16]
    );
    Ok(())
}

fn handle_event(
    event: ServiceEvent,
    my_fingerprint: &str,
    peers: &PeerMap,
    fullnames: &FullnameMap,
    prefs: &Arc<PreferencesStore>,
    manual: &Arc<ManualPeerStore>,
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
            emit_peers_changed(app, peers, prefs, manual);
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            let removed_id = {
                let mut f = fullnames.lock().unwrap();
                f.remove(&fullname)
            };
            if let Some(id) = removed_id {
                peers.lock().unwrap().remove(&id);
                emit_peers_changed(app, peers, prefs, manual);
            }
        }
        _ => {}
    }
}

fn emit_peers_changed(
    app: &AppHandle,
    peers: &PeerMap,
    prefs: &Arc<PreferencesStore>,
    manual: &Arc<ManualPeerStore>,
) {
    let snapshot = build_wire_list(peers, prefs, manual);
    let _ = app.emit("peers-changed", &snapshot);
}
