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
) {
    tauri::async_runtime::spawn(async move {
        run(app, info, peers, prefs, manual, aliases, clipboard).await;
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
) {
    let socket = match build_socket() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[udp] failed to bind UDP {}: {e:#}", UDP_DISCOVERY_PORT);
            return;
        }
    };
    eprintln!(
        "[udp] discovery active on 0.0.0.0:{} (announcing as {})",
        UDP_DISCOVERY_PORT, info.alias
    );

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
            eprintln!("[udp] serialize payload failed: {e}");
            return;
        }
    };

    let broadcast: SocketAddr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
        UDP_DISCOVERY_PORT,
    );
    let subnet_broadcast = derive_subnet_broadcast(&info.local_ip);

    let mut buf = vec![0u8; 4096];
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(BROADCAST_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(e) = socket.send_to(&bytes, broadcast).await {
                    eprintln!("[udp] broadcast send failed: {}", e);
                }
                if let Some(sb) = subnet_broadcast {
                    let _ = socket.send_to(&bytes, sb).await;
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
                        );
                    }
                    Err(e) => {
                        eprintln!("[udp] recv error: {}", e);
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

/// Given "192.168.1.42" → SocketAddr("192.168.1.255:53318").
fn derive_subnet_broadcast(local_ip: &str) -> Option<SocketAddr> {
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    // Assume /24 — by far the most common consumer LAN.
    Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
            255,
        )),
        UDP_DISCOVERY_PORT,
    ))
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
) {
    let pkt: DiscoveryPacket = match serde_json::from_slice(bytes) {
        Ok(p) => p,
        Err(_) => return,
    };

    if pkt.msg_type != MAGIC || pkt.fingerprint == my_fp || pkt.fingerprint.is_empty() {
        return;
    }

    let record = PeerRecord {
        id: pkt.fingerprint.clone(),
        name: pkt.alias,
        hex_id: pkt.hex_id,
        ip: peer_addr.ip().to_string(),
        port: pkt.tcp_port,
        icon_type: pkt.icon_type,
        last_seen: Instant::now(),
    };

    let was_new = {
        let mut p = peers.lock().unwrap();
        p.insert(record.id.clone(), record).is_none()
    };

    if was_new {
        eprintln!(
            "[udp] discovered {} via broadcast at {}",
            &pkt.fingerprint[..16.min(pkt.fingerprint.len())],
            peer_addr
        );
        let snapshot = build_wire_list(peers, prefs, manual, aliases, clipboard);
        let _ = app.emit("peers-changed", &snapshot);
    }
}
