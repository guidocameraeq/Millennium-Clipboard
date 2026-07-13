// Millennium Clipboard — UDP broadcast discovery (v0.8.0)
//
// mDNS multicast is unreliable on many real-world networks (AP isolation,
// IGMP snooping, multi-NIC bind issues, IPv6 quirks). UDP broadcast to
// 255.255.255.255 is dumb and trivially routed on every consumer LAN.
//
// Runs in parallel with mDNS. Each peer blasts a small JSON
// announcement over UDP/53318 every few seconds. The receiver pulls the
// fingerprint, alias and TCP port out of the payload and feeds them
// into the shared peer cache. The unified presence poller in
// `discovery.rs` then takes over with active TCP probes.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::net::UdpSocket;

use crate::aliases::AliasStore;
use crate::clipboard_sync::ClipboardSyncStore;
use crate::discovery::{build_wire_list, PeerMap, PeerRecord};
use crate::manual_peers::ManualPeerStore;
use crate::preferences::PreferencesStore;

pub const UDP_DISCOVERY_PORT: u16 = 53318;
const MAGIC: &str = "millennium-discovery";
const PROTOCOL_VERSION: u32 = 1;
const BROADCAST_INTERVAL_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryPacket {
    #[serde(rename = "type")]
    msg_type: String,
    version: u32,
    alias: String,
    fingerprint: String,
    hex_id: String,
    tcp_port: u16,
    icon_type: String,
}

#[derive(Clone)]
pub struct LocalInfo {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub tcp_port: u16,
    pub local_ip: String,
}

/// Spawn the UDP broadcaster + receiver. Must be called from a context
/// that has a tokio runtime available (Tauri's `setup` qualifies — the
/// tasks themselves spin up the socket lazily so we never touch tokio
/// I/O off-runtime).
pub fn spawn(
    app: AppHandle,
    info: LocalInfo,
    peers: PeerMap,
    prefs: Arc<PreferencesStore>,
    manual: Arc<ManualPeerStore>,
    aliases: Arc<AliasStore>,
    clipboard: Arc<ClipboardSyncStore>,
    icons: Arc<crate::icon_overrides::IconOverrideStore>,
) {
    tauri::async_runtime::spawn(async move {
        run(app, info, peers, prefs, manual, aliases, clipboard, icons).await;
    });
}

#[allow(clippy::too_many_arguments)]
async fn run(
    app: AppHandle,
    info: LocalInfo,
    peers: PeerMap,
    prefs: Arc<PreferencesStore>,
    manual: Arc<ManualPeerStore>,
    aliases: Arc<AliasStore>,
    clipboard: Arc<ClipboardSyncStore>,
    icons: Arc<crate::icon_overrides::IconOverrideStore>,
) {
    let socket = match build_socket() {
        Ok(s) => s,
        Err(e) => {
            crate::runtime_log::err(format!(
                "[udp] failed to bind UDP {}: {e:#}",
                UDP_DISCOVERY_PORT
            ));
            return;
        }
    };
    crate::runtime_log::info(format!(
        "[udp] discovery active on 0.0.0.0:{} (announcing as {} fp={})",
        UDP_DISCOVERY_PORT,
        info.alias,
        &info.fingerprint[..16.min(info.fingerprint.len())]
    ));

    let payload = DiscoveryPacket {
        msg_type: MAGIC.to_string(),
        version: PROTOCOL_VERSION,
        alias: info.alias.clone(),
        fingerprint: info.fingerprint.clone(),
        hex_id: info.hex_id.clone(),
        tcp_port: info.tcp_port,
        icon_type: if cfg!(target_os = "android") || cfg!(target_os = "ios") {
            "phone".to_string()
        } else {
            "desktop".to_string()
        },
    };
    let bytes = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            crate::runtime_log::err(format!("[udp] serialize payload failed: {e}"));
            return;
        }
    };

    let broadcast: SocketAddr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
        UDP_DISCOVERY_PORT,
    );
    // Limited broadcast (255.255.255.255) reaches every host on the local
    // segment regardless of the netmask and is never routed off-link. We no
    // longer derive a /24 directed broadcast: that hard-coded guess was
    // wrong on /16, /23, ... LANs. One global broadcast covers them all.
    crate::runtime_log::info(format!(
        "[udp] broadcasting to 255.255.255.255:{} every {}s (local_ip={})",
        UDP_DISCOVERY_PORT, BROADCAST_INTERVAL_SECS, info.local_ip
    ));

    let mut buf = vec![0u8; 4096];
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(BROADCAST_INTERVAL_SECS));
    // Skip missed ticks instead of firing a catch-up burst: if a recv or
    // send stalls past the interval, we want the next tick scheduled from
    // "now", not a pile of accumulated ticks all at once.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut send_count: u64 = 0;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                match socket.send_to(&bytes, broadcast).await {
                    Ok(_) => {
                        send_count += 1;
                        // Log every 12th tick (~1min) so the buffer doesn't drown.
                        if send_count % 12 == 1 {
                            crate::runtime_log::info(format!(
                                "[udp] still broadcasting (sent {} hellos so far)",
                                send_count
                            ));
                        }
                    }
                    Err(e) => {
                        crate::runtime_log::err(format!("[udp] broadcast send failed: {}", e));
                    }
                }
            }
            recv = socket.recv_from(&mut buf) => {
                match recv {
                    Ok((n, peer_addr)) => {
                        handle_packet(
                            &buf[..n],
                            peer_addr,
                            &info.fingerprint,
                            &app,
                            &peers,
                            &prefs,
                            &manual,
                            &aliases,
                            &clipboard,
                            &icons,
                        );
                    }
                    Err(e) => {
                        crate::runtime_log::err(format!("[udp] recv error: {}", e));
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}

fn build_socket() -> Result<UdpSocket> {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).context("create UDP socket")?;
    socket.set_reuse_address(true).context("SO_REUSEADDR")?;
    socket.set_broadcast(true).context("SO_BROADCAST")?;
    socket.set_nonblocking(true).context("set non-blocking")?;
    let bind_addr: SocketAddr =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), UDP_DISCOVERY_PORT);
    socket.bind(&bind_addr.into()).context("bind UDP 53318")?;
    let std_socket: std::net::UdpSocket = socket.into();
    // UdpSocket::from_std requires a Tokio runtime to be live on this
    // thread. spawn() callers must invoke us inside an async task.
    UdpSocket::from_std(std_socket).context("convert to tokio UdpSocket")
}

#[allow(clippy::too_many_arguments)]
fn handle_packet(
    bytes: &[u8],
    peer_addr: SocketAddr,
    my_fp: &str,
    app: &AppHandle,
    peers: &PeerMap,
    prefs: &Arc<PreferencesStore>,
    manual: &Arc<ManualPeerStore>,
    aliases: &Arc<AliasStore>,
    clipboard: &Arc<ClipboardSyncStore>,
    icons: &Arc<crate::icon_overrides::IconOverrideStore>,
) {
    let pkt: DiscoveryPacket = match serde_json::from_slice(bytes) {
        Ok(p) => p,
        Err(_) => return,
    };

    if pkt.msg_type != MAGIC || pkt.fingerprint == my_fp || pkt.fingerprint.is_empty() {
        return;
    }

    let fp_short = &pkt.fingerprint[..16.min(pkt.fingerprint.len())];
    let src_ip = peer_addr.ip().to_string();

    // `is_new`: a peer we'd never seen (first UDP sighting).
    // `should_emit`: the wire list changed — a new peer OR a corrected
    // route/alias on an existing one — so the frontend must be told.
    let (should_emit, is_new) = {
        let mut p = peers.lock().unwrap();
        match p.get_mut(&pkt.fingerprint) {
            Some(existing) => {
                existing.last_seen = Instant::now();
                // The datagram's source IP is authoritative: the kernel saw
                // it arrive, it can't be spoofed to a virtual NIC the way an
                // mDNS A-record can. So UDP always wins the route, confirmed
                // or not — this is what kills the asymmetric flap.
                existing.confirmed = true;
                let mut route_changed = false;
                if existing.ip != src_ip {
                    crate::runtime_log::info(format!(
                        "[udp] correcting IP for {}: {} -> {} (datagram src wins)",
                        fp_short, existing.ip, src_ip
                    ));
                    existing.ip = src_ip.clone();
                    route_changed = true;
                }
                if existing.port != pkt.tcp_port {
                    existing.port = pkt.tcp_port;
                    route_changed = true;
                }
                if existing.name != pkt.alias {
                    existing.name = pkt.alias.clone();
                    route_changed = true;
                }
                (route_changed, false)
            }
            None => {
                p.insert(
                    pkt.fingerprint.clone(),
                    PeerRecord {
                        id: pkt.fingerprint.clone(),
                        name: pkt.alias.clone(),
                        hex_id: pkt.hex_id.clone(),
                        ip: src_ip.clone(),
                        port: pkt.tcp_port,
                        icon_type: pkt.icon_type.clone(),
                        last_seen: Instant::now(),
                        // Source IP of the datagram is the real route.
                        confirmed: true,
                    },
                );
                (true, true)
            }
        }
    };

    if is_new {
        crate::runtime_log::info(format!(
            "[udp] NEW peer {} '{}' via broadcast from {} (payload tcp_port={})",
            fp_short, pkt.alias, peer_addr, pkt.tcp_port
        ));
    }
    if should_emit {
        let snapshot = build_wire_list(peers, prefs, manual, aliases, clipboard, icons);
        let _ = app.emit("peers-changed", &snapshot);
    }
}
