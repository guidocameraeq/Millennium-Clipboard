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
    /// The ip/port were confirmed by a real socket source: a TCP probe to
    /// /info, or the source IP of a UDP datagram. Once `true`, mDNS
    /// A-records (which can advertise a peer's virtual NICs) no longer
    /// overwrite the route. Reset implicitly when the record is reaped and
    /// re-learned from mDNS.
    pub confirmed: bool,
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
    pub icons: Arc<crate::icon_overrides::IconOverrideStore>,
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
    icons: &crate::icon_overrides::IconOverrideStore,
) -> Vec<WirePeer> {
    let online = peers.lock().unwrap();
    let mut result: Vec<WirePeer> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let apply_alias =
        |fp: &str, default: String| -> String { aliases.get(fp).unwrap_or(default) };
    let apply_icon =
        |fp: &str, default: String| -> String { icons.get(fp).unwrap_or(default) };

    for record in online.values() {
        let is_fav = prefs.is_favorite(&record.id);
        let is_manual = manual.contains(&record.id);
        let clip_sync = clipboard.is_enabled(&record.id);
        let mut wire = record.to_wire(is_fav, is_manual, clip_sync);
        wire.name = apply_alias(&record.id, wire.name);
        wire.icon_type = apply_icon(&record.id, wire.icon_type);
        result.push(wire);
        seen.insert(record.id.clone());
    }

    for m in manual.snapshot() {
        if seen.contains(&m.fingerprint) {
            continue;
        }
        let name = apply_alias(&m.fingerprint, m.alias);
        let icon = apply_icon(&m.fingerprint, m.icon_type);
        result.push(WirePeer {
            id: m.fingerprint.clone(),
            name,
            hex_id: m.hex_id,
            ip: m.ip,
            port: m.port,
            status: "offline",
            favorite: prefs.is_favorite(&m.fingerprint),
            icon_type: icon,
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
        let icon = apply_icon(&fav.fingerprint, fav.icon_type);
        result.push(WirePeer {
            id: fav.fingerprint.clone(),
            name,
            hex_id: fav.hex_id,
            ip: fav.last_ip,
            port: fav.last_port,
            status: "offline",
            favorite: true,
            icon_type: icon,
            manual: false,
            clipboard_sync: clipboard.is_enabled(&fav.fingerprint),
        });
    }
    result
}

impl DiscoveryState {
    pub fn peers_for_wire(&self) -> Vec<WirePeer> {
        build_wire_list(
            &self.peers,
            &self.prefs,
            &self.manual,
            &self.aliases,
            &self.clipboard,
            &self.icons,
        )
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
    icons: Arc<crate::icon_overrides::IconOverrideStore>,
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
    let icons_for_task = icons.clone();
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
                        &icons_for_task,
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
    // Presence (Fase 1): a cheap last_seen reaper + an on-demand TCP probe
    // scheduler. Replaces the old "TCP-probe every known peer every 6 s"
    // sweep, which was the main idle CPU/network cost and, combined with the
    // mDNS-vs-UDP IP disagreement, the source of the peer flapping.
    //
    // Liveness model:
    //   * A peer heard over UDP (every BROADCAST_INTERVAL_SECS) refreshes its
    //     last_seen for free — no TCP probe needed.
    //   * The reaper marks a peer offline once last_seen exceeds PEER_TTL
    //     (3x the UDP interval, so a lost hello or two doesn't flap it).
    //   * The probe scheduler only spends TCP on peers UDP is NOT keeping
    //     fresh: manual/favorite peers never heard live (with exponential
    //     backoff), and live peers going stale (legacy mDNS-only peers, or
    //     manual peers on a segment UDP broadcast can't cross).
    // ---------------------------------------------------------------------

    // (A) Reaper — no network, runs every 2 s. Drops peers whose last_seen
    //     expired and emits the updated wire list.
    {
        let peers = peers.clone();
        let fullnames = fullnames.clone();
        let prefs = prefs.clone();
        let manual = manual.clone();
        let aliases = aliases.clone();
        let clipboard = clipboard.clone();
        let icons = icons.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            use std::time::Duration;
            // Strictly greater than the UDP interval so a peer is never reaped
            // between two hellos; 3x leaves room for a lost hello.
            let peer_ttl = Duration::from_secs(
                crate::udp_discovery::BROADCAST_INTERVAL_SECS.saturating_mul(3),
            );
            let mut reap = tokio::time::interval(Duration::from_secs(2));
            reap.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            reap.tick().await; // skip the immediate first tick
            loop {
                reap.tick().await;
                let mut removed: Vec<String> = Vec::new();
                {
                    let mut p = peers.lock().unwrap();
                    p.retain(|_fp, rec| {
                        let alive = rec.last_seen.elapsed() < peer_ttl;
                        if !alive {
                            removed.push(rec.id.clone());
                        }
                        alive
                    });
                }
                if removed.is_empty() {
                    continue;
                }
                for fp in &removed {
                    crate::runtime_log::info(format!(
                        "[reaper] {} offline — no hello/probe for >{}s",
                        &fp[..16.min(fp.len())],
                        peer_ttl.as_secs()
                    ));
                }
                // Forget fullname → id mappings for the reaped peers so a
                // later ServiceResolved reattaches cleanly.
                {
                    let live: std::collections::HashSet<String> =
                        peers.lock().unwrap().keys().cloned().collect();
                    fullnames.lock().unwrap().retain(|_, id| live.contains(id));
                }
                let snapshot =
                    build_wire_list(&peers, &prefs, &manual, &aliases, &clipboard, &icons);
                let _ = app.emit("peers-changed", &snapshot);
            }
        });
    }

    // (B) Probe scheduler — runs every 2 s, but only probes peers UDP isn't
    //     keeping fresh, each on its own exponential backoff.
    {
        let peers = peers.clone();
        let prefs = prefs.clone();
        let manual = manual.clone();
        let aliases = aliases.clone();
        let clipboard = clipboard.clone();
        let icons = icons.clone();
        let app = app.clone();
        let my_fp = identity.fingerprint.clone();
        tauri::async_runtime::spawn(async move {
            use futures_util::future::join_all;
            use std::collections::HashMap as Map;
            use std::collections::HashSet;
            use std::time::{Duration, Instant};

            let udp_interval = crate::udp_discovery::BROADCAST_INTERVAL_SECS;
            // Re-probe a live peer only once it's older than one UDP interval
            // + a margin: above the UDP cadence so healthy UDP peers are never
            // probed, below PEER_TTL so probe-only peers get refreshed before
            // the reaper drops them.
            let reprobe_after = Duration::from_secs(udp_interval + 1);
            let min_backoff = Duration::from_secs(6);
            let max_backoff = Duration::from_secs(300);

            // Per-absent-peer next-probe time + current backoff. Both are
            // purged each tick of any fp that stopped being a candidate, so
            // neither map can grow without bound.
            let mut probe_at: Map<String, Instant> = Map::new();
            let mut backoff: Map<String, Duration> = Map::new();

            let mut sched = tokio::time::interval(Duration::from_secs(2));
            sched.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            sched.tick().await; // skip the immediate first tick

            loop {
                sched.tick().await;
                let now = Instant::now();

                // Snapshot live peers with their route + staleness.
                let live: Map<String, (String, u16, String, String, Duration)> = peers
                    .lock()
                    .unwrap()
                    .values()
                    .map(|r| {
                        (
                            r.id.clone(),
                            (
                                r.ip.clone(),
                                r.port,
                                r.hex_id.clone(),
                                r.icon_type.clone(),
                                r.last_seen.elapsed(),
                            ),
                        )
                    })
                    .collect();

                // Candidate set: fp -> the route we'd probe.
                let mut candidates: Map<String, (String, u16, String, String)> = Map::new();
                // 1. Live peers going stale (UDP isn't refreshing them).
                for (fp, (ip, port, hex, icon, elapsed)) in &live {
                    if *fp == my_fp {
                        continue;
                    }
                    if *elapsed > reprobe_after {
                        candidates
                            .insert(fp.clone(), (ip.clone(), *port, hex.clone(), icon.clone()));
                    }
                }
                // 2. Manual/favorite peers never heard live. `or_insert` keeps
                //    a live-stale peer's confirmed route over the stored one.
                for m in manual.snapshot() {
                    if m.fingerprint == my_fp || live.contains_key(&m.fingerprint) {
                        continue;
                    }
                    candidates
                        .entry(m.fingerprint)
                        .or_insert((m.ip, m.port, m.hex_id, m.icon_type));
                }
                for f in prefs.favorites_snapshot() {
                    if f.fingerprint == my_fp || live.contains_key(&f.fingerprint) {
                        continue;
                    }
                    candidates
                        .entry(f.fingerprint)
                        .or_insert((f.last_ip, f.last_port, f.hex_id, f.icon_type));
                }

                // Purge schedule/backoff for fps that are no longer candidates.
                let candidate_fps: HashSet<&String> = candidates.keys().collect();
                probe_at.retain(|fp, _| candidate_fps.contains(fp));
                backoff.retain(|fp, _| candidate_fps.contains(fp));

                // Of the candidates, which are due now?
                let mut due: Vec<(String, String, u16, String, String)> = Vec::new();
                for (fp, (ip, port, hex, icon)) in &candidates {
                    let at = probe_at.entry(fp.clone()).or_insert(now);
                    if *at <= now {
                        due.push((fp.clone(), ip.clone(), *port, hex.clone(), icon.clone()));
                    }
                }
                if due.is_empty() {
                    continue;
                }

                let probes: Vec<_> = due
                    .into_iter()
                    .map(|(fp, ip, port, hex, icon)| async move {
                        let res = tokio::time::timeout(
                            Duration::from_secs(5),
                            crate::http_client::fetch_info(&ip, port),
                        )
                        .await;
                        (fp, ip, port, hex, icon, res)
                    })
                    .collect();
                let results = join_all(probes).await;

                let mut changed = false;
                for (fp, probed_ip, probed_port, hex, icon, res) in results {
                    let fp_short = &fp[..16.min(fp.len())];
                    match res {
                        Ok(Ok(info)) if info.fingerprint == fp => {
                            // Reachable + right identity: clear the backoff.
                            backoff.remove(&fp);
                            probe_at.remove(&fp);
                            let mut p = peers.lock().unwrap();
                            match p.get_mut(&fp) {
                                Some(existing) => {
                                    // Liveness refresh only — never clobber the
                                    // stored route (UDP's datagram-src IP wins).
                                    existing.last_seen = Instant::now();
                                    existing.confirmed = true;
                                    if existing.name != info.alias {
                                        existing.name = info.alias;
                                        changed = true;
                                    }
                                }
                                None => {
                                    p.insert(
                                        fp.clone(),
                                        PeerRecord {
                                            id: fp.clone(),
                                            name: info.alias,
                                            hex_id: hex,
                                            ip: probed_ip.clone(),
                                            port: probed_port,
                                            icon_type: icon,
                                            last_seen: Instant::now(),
                                            confirmed: true,
                                        },
                                    );
                                    drop(p);
                                    crate::runtime_log::info(format!(
                                        "[probe] first sight {} @ {}:{} (via probe)",
                                        fp_short, probed_ip, probed_port
                                    ));
                                    changed = true;
                                }
                            }
                        }
                        Ok(Ok(info)) => {
                            // Someone else answers at that IP:port now.
                            crate::runtime_log::warn(format!(
                                "[probe] DRIFT {}:{} expected={} got={} — dropping",
                                probed_ip,
                                probed_port,
                                fp_short,
                                &info.fingerprint[..16.min(info.fingerprint.len())]
                            ));
                            if peers.lock().unwrap().remove(&fp).is_some() {
                                changed = true;
                            }
                            let entry = backoff.entry(fp.clone()).or_insert(min_backoff);
                            let this = *entry;
                            probe_at.insert(fp.clone(), now + this);
                            *entry = (this * 2).min(max_backoff);
                        }
                        _ => {
                            // Unreachable / timed out. The reaper owns removal
                            // by last_seen; here we only widen the backoff so an
                            // absent peer isn't hammered every couple seconds.
                            let entry = backoff.entry(fp.clone()).or_insert(min_backoff);
                            let this = *entry;
                            probe_at.insert(fp.clone(), now + this);
                            *entry = (this * 2).min(max_backoff);
                        }
                    }
                }

                if changed {
                    let snapshot =
                        build_wire_list(&peers, &prefs, &manual, &aliases, &clipboard, &icons);
                    let _ = app.emit("peers-changed", &snapshot);
                }
            }
        });
    }

    Ok(DiscoveryState { peers, fullnames, prefs, manual, aliases, clipboard, icons, daemon })
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

/// Reconcile an incoming mDNS resolve against an existing `PeerRecord`.
///
/// The single reconciliation policy for mDNS: metadata (name/hex/icon)
/// always refreshes because it's harmless labelling, but the ROUTE
/// (ip/port) only updates while the record is NOT `confirmed`. A confirmed
/// record was proven by a UDP datagram source IP or a TCP probe, and mDNS
/// A-records (which advertise every NIC of the peer, WSL/Hyper-V included)
/// must never clobber it. Returns whether anything that reaches the wire
/// list changed. Pure (no logging / no lock) so it can be unit-tested.
fn reconcile_mdns(
    existing: &mut PeerRecord,
    alias: &str,
    hex_id: &str,
    icon_type: &str,
    ip: &str,
    port: u16,
) -> bool {
    existing.last_seen = Instant::now();
    let meta_changed = existing.name != alias
        || existing.hex_id != hex_id
        || existing.icon_type != icon_type;
    let route_changed =
        if !existing.confirmed && (existing.ip != ip || existing.port != port) {
            existing.ip = ip.to_string();
            existing.port = port;
            true
        } else {
            false
        };
    if meta_changed {
        existing.name = alias.to_string();
        existing.hex_id = hex_id.to_string();
        existing.icon_type = icon_type.to_string();
    }
    meta_changed || route_changed
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
    icons: &Arc<crate::icon_overrides::IconOverrideStore>,
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
                        let old_ip = existing.ip.clone();
                        let old_port = existing.port;
                        let was_confirmed = existing.confirmed;
                        let route_differs = old_ip != ip || old_port != port;
                        let changed =
                            reconcile_mdns(existing, &alias, &hex_id, &icon_type, &ip, port);
                        if route_differs {
                            if was_confirmed {
                                // mDNS wants to move a confirmed peer (probably to
                                // one of its virtual NICs). We keep the real route.
                                crate::runtime_log::info(format!(
                                    "[mdns] ignoring A-record {}:{} for confirmed peer {} (keeping {}:{})",
                                    ip, port, fp_short, old_ip, old_port
                                ));
                            } else {
                                crate::runtime_log::info(format!(
                                    "[mdns] resolve {} '{}' route (unconfirmed) {}:{} -> {}:{} (announced addrs: {:?})",
                                    fp_short, alias, old_ip, old_port, ip, port, all_addrs
                                ));
                            }
                        }
                        changed
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
                                // mDNS is a "first sight" hint only: the A-record
                                // may point at a virtual NIC. Not confirmed until
                                // a UDP datagram or a TCP probe proves the route.
                                confirmed: false,
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
                emit_peers_changed(app, peers, prefs, manual, aliases, clipboard, icons);
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
    icons: &Arc<crate::icon_overrides::IconOverrideStore>,
) {
    let snapshot = build_wire_list(peers, prefs, manual, aliases, clipboard, icons);
    let _ = app.emit("peers-changed", &snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ip: &str, port: u16, confirmed: bool) -> PeerRecord {
        PeerRecord {
            id: "fp".into(),
            name: "ALPHA".into(),
            hex_id: "0xAB:CD:EF".into(),
            ip: ip.into(),
            port,
            icon_type: "desktop".into(),
            last_seen: Instant::now(),
            confirmed,
        }
    }

    #[test]
    fn mdns_never_overwrites_a_confirmed_route() {
        // Peer proven at 192.168.1.42 (e.g. by a UDP datagram). mDNS then
        // resolves the same peer to its WSL NIC (172.20.0.1). The route
        // must NOT move.
        let mut r = rec("192.168.1.42", 53319, true);
        let changed = reconcile_mdns(&mut r, "ALPHA", "0xAB:CD:EF", "desktop", "172.20.0.1", 53319);
        assert_eq!(r.ip, "192.168.1.42", "confirmed route preserved");
        assert_eq!(r.port, 53319);
        assert!(!changed, "no wire-visible change");
    }

    #[test]
    fn mdns_updates_route_of_unconfirmed_peer() {
        // A peer only ever seen by mDNS (unconfirmed) may still have its
        // route corrected by a later, better mDNS resolve.
        let mut r = rec("192.168.1.42", 53319, false);
        let changed = reconcile_mdns(&mut r, "ALPHA", "0xAB:CD:EF", "desktop", "192.168.1.99", 53320);
        assert_eq!(r.ip, "192.168.1.99");
        assert_eq!(r.port, 53320);
        assert!(changed);
    }

    #[test]
    fn mdns_refreshes_metadata_even_on_confirmed_peer() {
        // Alias/icon are labelling, not routing: they refresh regardless of
        // `confirmed`, but the ip/port stay put.
        let mut r = rec("192.168.1.42", 53319, true);
        let changed = reconcile_mdns(&mut r, "BRAVO", "0x11:22:33", "phone", "172.20.0.1", 53319);
        assert_eq!(r.name, "BRAVO");
        assert_eq!(r.icon_type, "phone");
        assert_eq!(r.ip, "192.168.1.42", "route still preserved");
        assert!(changed, "metadata change is wire-visible");
    }
}
