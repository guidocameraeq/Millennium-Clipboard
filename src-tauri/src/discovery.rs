// Millennium Clipboard — discovery (Fase 3 + Fase 4)
//
// mDNS service registration and browsing. Identity now comes from
// `identity.rs` (cert fingerprint), no longer a per-run UUID. TXT
// records advertise the fingerprint so peers can verify the cert they
// later see at /info matches what the discovery announced.

use crate::aliases::AliasStore;
use crate::clipboard_sync::ClipboardSyncStore;
use crate::identity::Identity;
use crate::manual_peers::ManualPeerStore;
use crate::preferences::PreferencesStore;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo};
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
    pub clipboard_sync: bool,
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
    pub fn to_wire(&self, favorite: bool, manual: bool, clipboard_sync: bool) -> WirePeer {
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
            clipboard_sync,
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
    pub aliases: Arc<AliasStore>,
    pub clipboard: Arc<ClipboardSyncStore>,
    #[allow(dead_code)]
    pub daemon: ServiceDaemon,
}

/// Build the merged wire-list. Sources in priority order:
///   1. Peers currently visible on mDNS (status = online).
///   2. Manually-added peers we couldn't reach (status = offline, manual = true).
///   3. Favorite peers we haven't seen in this session (status = offline).
pub(crate) fn build_wire_list(
    peers: &PeerMap,
    prefs: &PreferencesStore,
    manual: &ManualPeerStore,
    aliases: &AliasStore,
    clipboard: &ClipboardSyncStore,
) -> Vec<WirePeer> {
    let online = peers.lock().unwrap();
    let mut result: Vec<WirePeer> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let apply_alias =
        |fp: &str, default: String| -> String { aliases.get(fp).unwrap_or(default) };

    for record in online.values() {
        let is_fav = prefs.is_favorite(&record.id);
        let is_manual = manual.contains(&record.id);
        let clip_sync = clipboard.is_enabled(&record.id);
        let mut wire = record.to_wire(is_fav, is_manual, clip_sync);
        wire.name = apply_alias(&record.id, wire.name);
        result.push(wire);
        seen.insert(record.id.clone());
    }

    for m in manual.snapshot() {
        if seen.contains(&m.fingerprint) {
            continue;
        }
        let name = apply_alias(&m.fingerprint, m.alias);
        result.push(WirePeer {
            id: m.fingerprint.clone(),
            name,
            hex_id: m.hex_id,
            ip: m.ip,
            port: m.port,
            status: "offline",
            favorite: prefs.is_favorite(&m.fingerprint),
            icon_type: m.icon_type,
            manual: true,
            clipboard_sync: clipboard.is_enabled(&m.fingerprint),
        });
        seen.insert(m.fingerprint);
    }

    for fav in prefs.favorites_snapshot() {
        if seen.contains(&fav.fingerprint) {
            continue;
        }
        let name = apply_alias(&fav.fingerprint, fav.alias);
        result.push(WirePeer {
            id: fav.fingerprint.clone(),
            name,
            hex_id: fav.hex_id,
            ip: fav.last_ip,
            port: fav.last_port,
            status: "offline",
            favorite: true,
            icon_type: fav.icon_type,
            manual: false,
            clipboard_sync: clipboard.is_enabled(&fav.fingerprint),
        });
    }
    result
}

impl DiscoveryState {
    pub fn peers_for_wire(&self) -> Vec<WirePeer> {
        build_wire_list(&self.peers, &self.prefs, &self.manual, &self.aliases, &self.clipboard)
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
    aliases: Arc<AliasStore>,
    clipboard: Arc<ClipboardSyncStore>,
) -> Result<DiscoveryState, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;

    // Add the resolved local IP as an explicit interface, but keep the
    // mdns-sd default (every IPv4 interface) active as a fallback.
    // Previously we did disable_interface(All) + enable(local_ip),
    // which on machines with WSL/Hyper-V/Docker/VPN routinely picked
    // the wrong NIC (because local_ip_address::local_ip() returns the
    // lowest-metric route, often a virtual one). Result: announcements
    // went out the virtual switch and no peer ever saw us.
    if let Ok(local_ip) = identity.local_ip.parse::<std::net::IpAddr>() {
        match daemon.enable_interface(IfKind::Addr(local_ip)) {
            Ok(_) => crate::runtime_log::info(format!(
                "[mdns] explicitly enabled interface {}",
                local_ip
            )),
            Err(e) => crate::runtime_log::warn(format!(
                "[mdns] enable_interface({}) failed (keeping defaults): {}",
                local_ip, e
            )),
        }
    } else {
        crate::runtime_log::warn(format!(
            "[mdns] could not parse local_ip='{}', using default bind",
            identity.local_ip
        ));
    }

    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let fullnames: FullnameMap = Arc::new(Mutex::new(HashMap::new()));

    register_self(&daemon, identity, port)?;

    let receiver = daemon.browse(SERVICE_TYPE)?;

    let peers_for_task = peers.clone();
    let fullnames_for_task = fullnames.clone();
    let prefs_for_task = prefs.clone();
    let manual_for_task = manual.clone();
    let aliases_for_task = aliases.clone();
    let clipboard_for_task = clipboard.clone();
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
                        &aliases_for_task,
                        &clipboard_for_task,
                        &app_handle,
                    );
                }
                Err(e) => {
                    crate::runtime_log::err(format!("[mdns] channel closed: {}", e));
                    break;
                }
            }
        }
    });

    // ---------------------------------------------------------------------
    // Unified presence poller (v0.7.0)
    //
    // mDNS multicast is unreliable on Wi-Fi with AP isolation, IGMP
    // snooping, firewall quirks, or just packet loss. So we treat mDNS
    // as a "first sight" mechanism and use a TCP probe of /info as the
    // source of truth for online/offline.
    //
    // Every ~6 s we probe in parallel every peer we know about (from
    // mDNS cache, manual entries, or stored favorites). If they reply
    // with the expected fingerprint, they're online and we refresh the
    // cache. Two consecutive failures → drop from live cache. Manual
    // and favorite peers still show as offline in `build_wire_list`.
    // ---------------------------------------------------------------------
    let peers_for_poll = peers.clone();
    let fullnames_for_poll = fullnames.clone();
    let prefs_for_poll = prefs.clone();
    let manual_for_poll = manual.clone();
    let aliases_for_poll = aliases.clone();
    let clipboard_for_poll = clipboard.clone();
    let app_for_poll = app.clone();
    let daemon_for_poll = daemon.clone();
    let my_fp_poll = identity.fingerprint.clone();

    tauri::async_runtime::spawn(async move {
        use futures_util::future::join_all;
        use std::collections::HashMap as Map;
        use std::time::Duration;

        let mut failures: Map<String, u8> = Map::new();
        let mut tick = tokio::time::interval(Duration::from_secs(6));
        tick.tick().await; // skip the immediate first tick

        loop {
            tick.tick().await;

            // Still poke mDNS so any newcomer that's announcing is heard.
            let _ = daemon_for_poll.browse(SERVICE_TYPE);

            // Build the candidate set, preferring mDNS metadata over
            // manual/favorite stored data (mDNS has the freshest alias).
            let mut by_fp: Map<String, (String, u16, String, String)> = Map::new();
            for r in peers_for_poll.lock().unwrap().values() {
                by_fp.insert(
                    r.id.clone(),
                    (r.ip.clone(), r.port, r.hex_id.clone(), r.icon_type.clone()),
                );
            }
            for m in manual_for_poll.snapshot() {
                by_fp
                    .entry(m.fingerprint.clone())
                    .or_insert((m.ip, m.port, m.hex_id, m.icon_type));
            }
            for f in prefs_for_poll.favorites_snapshot() {
                by_fp
                    .entry(f.fingerprint.clone())
                    .or_insert((f.last_ip, f.last_port, f.hex_id, f.icon_type));
            }
            by_fp.remove(&my_fp_poll);

            if by_fp.is_empty() {
                continue;
            }

            // Probe everyone in parallel with a tight per-peer timeout.
            let probes: Vec<_> = by_fp
                .into_iter()
                .map(|(fp, (ip, port, hex_id, icon_type))| async move {
                    let res = tokio::time::timeout(
                        Duration::from_secs(5),
                        crate::http_client::fetch_info(&ip, port),
                    )
                    .await;
                    (fp, ip, port, hex_id, icon_type, res)
                })
                .collect();
            let results = join_all(probes).await;

            let mut changed = false;
            for (fp, ip, port, hex_id, icon_type, res) in results {
                let fp_short = &fp[..16.min(fp.len())];
                match res {
                    Ok(Ok(info)) if info.fingerprint == fp => {
                        let prev_failures = failures.remove(&fp).unwrap_or(0);
                        if prev_failures > 0 {
                            crate::runtime_log::info(format!(
                                "[poll] OK {} @ {}:{} (recovered after {} fail(s))",
                                fp_short, ip, port, prev_failures
                            ));
                        }
                        let record = PeerRecord {
                            id: fp.clone(),
                            name: info.alias,
                            hex_id,
                            ip: ip.clone(),
                            port,
                            icon_type,
                            last_seen: std::time::Instant::now(),
                        };
                        let was_new = peers_for_poll
                            .lock()
                            .unwrap()
                            .insert(fp.clone(), record)
                            .is_none();
                        if was_new {
                            crate::runtime_log::info(format!(
                                "[poll] first sight {} @ {}:{} (via probe)",
                                fp_short, ip, port
                            ));
                            changed = true;
                        }
                    }
                    Ok(Ok(info)) => {
                        // Fingerprint drift — someone else now answers at that IP:port.
                        crate::runtime_log::warn(format!(
                            "[poll] DRIFT {}:{} expected={} got={} — dropping",
                            ip,
                            port,
                            fp_short,
                            &info.fingerprint[..16.min(info.fingerprint.len())]
                        ));
                        if peers_for_poll.lock().unwrap().remove(&fp).is_some() {
                            changed = true;
                        }
                    }
                    Ok(Err(e)) => {
                        let count = failures.entry(fp.clone()).or_insert(0);
                        *count += 1;
                        crate::runtime_log::warn(format!(
                            "[poll] probe failed {} @ {}:{} ({}/3): {}",
                            fp_short, ip, port, count, e
                        ));
                        if *count >= 3
                            && peers_for_poll.lock().unwrap().remove(&fp).is_some()
                        {
                            changed = true;
                            crate::runtime_log::err(format!(
                                "[poll] DROPPED {} @ {}:{} after 3 failed probes",
                                fp_short, ip, port
                            ));
                        }
                    }
                    Err(_) => {
                        let count = failures.entry(fp.clone()).or_insert(0);
                        *count += 1;
                        crate::runtime_log::warn(format!(
                            "[poll] probe TIMEOUT {} @ {}:{} ({}/3)",
                            fp_short, ip, port, count
                        ));
                        if *count >= 3
                            && peers_for_poll.lock().unwrap().remove(&fp).is_some()
                        {
                            changed = true;
                            crate::runtime_log::err(format!(
                                "[poll] DROPPED {} @ {}:{} after 3 timeouts",
                                fp_short, ip, port
                            ));
                        }
                    }
                }
            }

            if changed {
                let live_ids: std::collections::HashSet<String> =
                    peers_for_poll.lock().unwrap().keys().cloned().collect();
                fullnames_for_poll
                    .lock()
                    .unwrap()
                    .retain(|_, id| live_ids.contains(id));

                let snapshot = build_wire_list(
                    &peers_for_poll,
                    &prefs_for_poll,
                    &manual_for_poll,
                    &aliases_for_poll,
                    &clipboard_for_poll,
                );
                let _ = app_for_poll.emit("peers-changed", &snapshot);
            }
        }
    });

    Ok(DiscoveryState { peers, fullnames, prefs, manual, aliases, clipboard, daemon })
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

/// Strip the alias down to characters that are legal in a DNS label
/// (ASCII alphanumeric plus `-`). Empty result falls back to "host".
fn sanitize_hostname(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "host".to_string()
    } else {
        trimmed.to_lowercase()
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
    // mDNS hostnames must be ASCII alphanumeric (plus `-`). Strip
    // anything else from the user alias before using it.
    let host = sanitize_hostname(&id.alias);

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &format!("{}.local.", host),
        id.local_ip.as_str(),
        port,
        Some(props),
    )?;
    daemon.register(service)?;
    crate::runtime_log::info(format!(
        "[mdns] registered {} on {}:{} (fp={})",
        instance_name,
        id.local_ip,
        port,
        &id.fingerprint[..16]
    ));
    Ok(())
}

fn handle_event(
    event: ServiceEvent,
    my_fingerprint: &str,
    peers: &PeerMap,
    fullnames: &FullnameMap,
    prefs: &Arc<PreferencesStore>,
    manual: &Arc<ManualPeerStore>,
    aliases: &Arc<AliasStore>,
    clipboard: &Arc<ClipboardSyncStore>,
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

            let port = info.get_port();
            let fullname = info.get_fullname().to_string();
            let all_addrs: Vec<String> = info
                .get_addresses()
                .iter()
                .map(|a| a.to_string())
                .collect();
            let fp_short = &id[..16.min(id.len())];

            let changed = {
                let mut p = peers.lock().unwrap();
                match p.get_mut(&id) {
                    Some(existing) => {
                        let same = existing.name == alias
                            && existing.hex_id == hex_id
                            && existing.ip == ip
                            && existing.port == port
                            && existing.icon_type == icon_type;
                        existing.last_seen = Instant::now();
                        if !same {
                            crate::runtime_log::info(format!(
                                "[mdns] resolve {} '{}' changed: ip {}->{} port {}->{} (announced addrs: {:?})",
                                fp_short, alias, existing.ip, ip, existing.port, port, all_addrs
                            ));
                            existing.name = alias;
                            existing.hex_id = hex_id;
                            existing.ip = ip;
                            existing.port = port;
                            existing.icon_type = icon_type;
                        }
                        !same
                    }
                    None => {
                        crate::runtime_log::info(format!(
                            "[mdns] resolve {} '{}' NEW @ {}:{} (announced addrs: {:?})",
                            fp_short, alias, ip, port, all_addrs
                        ));
                        p.insert(
                            id.clone(),
                            PeerRecord {
                                id: id.clone(),
                                name: alias,
                                hex_id,
                                ip,
                                port,
                                icon_type,
                                last_seen: Instant::now(),
                            },
                        );
                        true
                    }
                }
            };
            {
                let mut f = fullnames.lock().unwrap();
                f.insert(fullname, id);
            }
            if changed {
                emit_peers_changed(app, peers, prefs, manual, aliases, clipboard);
            }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            // mDNS is unreliable on multicast — ServiceRemoved fires on TTL
            // expiry even when the peer is perfectly alive. The TCP-probe
            // poller is our source of truth for liveness; let it decide
            // when a peer goes offline. Here we only forget the fullname
            // mapping so the next ServiceResolved can reattach cleanly.
            let mut f = fullnames.lock().unwrap();
            if f.remove(&fullname).is_some() {
                crate::runtime_log::info(format!(
                    "[mdns] ServiceRemoved '{}' (fullname forgotten; TCP poller still owns liveness)",
                    fullname
                ));
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
    aliases: &Arc<AliasStore>,
    clipboard: &Arc<ClipboardSyncStore>,
) {
    let snapshot = build_wire_list(peers, prefs, manual, aliases, clipboard);
    let _ = app.emit("peers-changed", &snapshot);
}
