// Millennium Clipboard — backend (Fase 7)
//
// Wires identity, persisted prefs/settings, HTTPS server, mDNS discovery,
// and the HTTPS client used to talk to peers. Commands invoked from JS
// are at the bottom.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use uuid::Uuid;

mod aliases;
mod clipboard_sync;
mod discovery;
mod http_client;
mod http_server;
mod icon_overrides;
mod identity;
mod json_store;
mod manual_peers;
mod preferences;
mod runtime_log;
mod settings;
mod thumbnails;
mod udp_discovery;
mod updater;
#[cfg(target_os = "android")]
mod android_fs_bridge;
#[cfg(target_os = "windows")]
mod windows_integration;

// ---------------------------------------------------------------------------
// Wire types shared with the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalInfo {
    pub alias: String,
    pub host_id_hex: String,
    pub ip: String,
    pub port: u16,
    pub fingerprint: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub download_dir: String,
    pub auto_accept_favorites: bool,
    pub notifications_enabled: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    discovery: discovery::DiscoveryState,
    identity: identity::Identity,
    prefs: Arc<preferences::PreferencesStore>,
    settings: Arc<settings::SettingsStore>,
    manual: Arc<manual_peers::ManualPeerStore>,
    aliases: Arc<aliases::AliasStore>,
    clipboard: Arc<clipboard_sync::ClipboardSyncStore>,
    icons: Arc<icon_overrides::IconOverrideStore>,
    server_port: u16,
}

// ---------------------------------------------------------------------------
// Identity / peers commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_local_info(state: tauri::State<AppState>) -> LocalInfo {
    let id = &state.identity;
    LocalInfo {
        alias: id.alias.clone(),
        host_id_hex: id.hex_id.clone(),
        // Live IP (updated on a network roam), not the boot-time one.
        ip: state.discovery.current_local_ip(),
        port: state.server_port,
        fingerprint: id.fingerprint.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[tauri::command]
fn list_peers(state: tauri::State<AppState>) -> Vec<discovery::WirePeer> {
    state.discovery.peers_for_wire()
}

#[tauri::command]
fn rescan_peers(state: tauri::State<AppState>) -> Result<Vec<discovery::WirePeer>, String> {
    discovery::rebrowse(&state.discovery).map_err(|e| e.to_string())?;
    // Also kick the probe scheduler so absent manual/favorite peers get
    // re-probed now instead of waiting out their backoff (up to 5 min).
    state.discovery.wake_probes();
    Ok(state.discovery.peers_for_wire())
}

#[tauri::command]
fn toggle_favorite(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
    value: bool,
) -> Result<(), String> {
    if value {
        let fav = state
            .discovery
            .favorite_from_peer(&peer_id)
            .ok_or_else(|| format!("peer {} is not on the grid right now", &peer_id[..8]))?;
        state.prefs.add_favorite(fav).map_err(|e| format!("{e:#}"))?;
    } else {
        state.prefs.remove_favorite(&peer_id).map_err(|e| format!("{e:#}"))?;
    }
    println!("[backend] toggle_favorite → peer={} value={}", peer_id, value);
    state.discovery.emit_snapshot(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Text transfer (Fase 5)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn send_text(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    text: String,
) -> Result<(), String> {
    let target = state
        .discovery
        .peers
        .lock()
        .unwrap()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| format!("peer {} not on the grid", peer_id))?;

    println!(
        "[backend] send_text → peer={} ({}:{}) chars={}",
        target.name,
        target.ip,
        target.port,
        text.chars().count()
    );

    let remote = http_client::fetch_info(&target.ip, target.port)
        .await
        .map_err(|e| format!("identity probe failed: {e:#}"))?;
    if remote.fingerprint != peer_id {
        return Err(format!(
            "fingerprint mismatch — expected {}, got {}",
            &peer_id[..16],
            &remote.fingerprint[..16]
        ));
    }

    http_client::post_text(
        &target.ip,
        target.port,
        text,
        state.identity.alias.clone(),
        state.identity.fingerprint.clone(),
        state.server_port,
    )
    .await
    .map_err(|e| format!("send failed: {e:#}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// File transfer (Fase 7)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn send_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    peer_id: String,
    file_paths: Vec<String>,
) -> Result<String, String> {
    if file_paths.is_empty() {
        return Err("no files to send".into());
    }

    let target = state
        .discovery
        .peers
        .lock()
        .unwrap()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| format!("peer {} not on the grid", peer_id))?;

    // Verify identity (Fase 5 cross-check, reused).
    let remote = http_client::fetch_info(&target.ip, target.port)
        .await
        .map_err(|e| format!("identity probe failed: {e:#}"))?;
    if remote.fingerprint != peer_id {
        return Err("fingerprint mismatch — peer changed identity".into());
    }

    // Gather metadata for each file.
    let session_id = Uuid::new_v4().simple().to_string();
    let mut prepare_files: Vec<http_client::PrepareFile> = Vec::new();
    let mut upload_plan: Vec<(String, PathBuf, u64)> = Vec::new();

    for path_str in &file_paths {
        let p = PathBuf::from(path_str);
        let meta = tokio::fs::metadata(&p)
            .await
            .map_err(|e| format!("stat {}: {}", p.display(), e))?;
        if !meta.is_file() {
            return Err(format!(
                "{} is not a regular file (folder transfer arrives later)",
                p.display()
            ));
        }
        let size = meta.len();
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("invalid file name: {}", p.display()))?;
        let mime = mime_guess::from_path(&p)
            .first()
            .map(|m| m.essence_str().to_string());
        let file_id = Uuid::new_v4().simple().to_string();
        // Best-effort thumbnail. Failures (corrupt image, unsupported
        // format, oversize) just produce None and the receiver shows a
        // generic icon — never blocks the transfer.
        let thumbnail = thumbnails::generate_for(&p, size).unwrap_or(None);
        prepare_files.push(http_client::PrepareFile {
            file_id: file_id.clone(),
            name: name.clone(),
            size,
            mime,
            sha256: None, // MVP: skip hashing big files; add in Fase 8 polish
            rel_path: None,
            thumbnail,
        });
        upload_plan.push((file_id, p, size));
    }

    println!(
        "[backend] send_files → peer={} ({}:{}) files={} session={}",
        target.name,
        target.ip,
        target.port,
        upload_plan.len(),
        &session_id[..8]
    );

    // Ask the peer; this blocks until the user accepts/rejects/times out.
    let prep = http_client::prepare_upload(
        &target.ip,
        target.port,
        &session_id,
        &state.identity.alias,
        &state.identity.fingerprint,
        state.server_port,
        &prepare_files,
    )
    .await
    .map_err(|e| format!("prepare: {e:#}"))?;

    // Upload each file (sequential; same Client → keep-alive).
    for (file_id, path, size) in &upload_plan {
        let token = prep
            .files
            .get(file_id)
            .ok_or_else(|| format!("no token for {}", &file_id[..8]))?;
        http_client::upload_file(
            app.clone(),
            &target.ip,
            target.port,
            &session_id,
            file_id,
            token,
            path,
            *size,
        )
        .await
        .map_err(|e| format!("upload {}: {e:#}", path.display()))?;
    }

    Ok(session_id)
}

#[tauri::command]
fn approve_session(session_id: String) -> Result<(), String> {
    if http_server::resolve_approval(&session_id, true) {
        Ok(())
    } else {
        Err("no pending session with that id".into())
    }
}

#[tauri::command]
fn reject_session(session_id: String) -> Result<(), String> {
    if http_server::resolve_approval(&session_id, false) {
        Ok(())
    } else {
        Err("no pending session with that id".into())
    }
}

#[tauri::command]
async fn cancel_session(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    session_id: String,
) -> Result<(), String> {
    let target = state
        .discovery
        .peers
        .lock()
        .unwrap()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| format!("peer {} not on the grid", peer_id))?;
    http_client::cancel_upload(&target.ip, target.port, &session_id)
        .await
        .map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------------------
// Settings commands (Fase 7)
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> UserSettings {
    let s = state.settings.snapshot();
    UserSettings {
        download_dir: s.download_dir.to_string_lossy().to_string(),
        auto_accept_favorites: s.auto_accept_favorites,
        notifications_enabled: s.notifications_enabled,
        start_with_windows: s.start_with_windows,
        close_to_tray: s.close_to_tray,
    }
}

#[tauri::command]
fn set_download_dir(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    state
        .settings
        .set_download_dir(PathBuf::from(path))
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_auto_accept_favorites(
    state: tauri::State<AppState>,
    value: bool,
) -> Result<(), String> {
    state
        .settings
        .set_auto_accept_favorites(value)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_notifications_enabled(
    state: tauri::State<AppState>,
    value: bool,
) -> Result<(), String> {
    state
        .settings
        .set_notifications_enabled(value)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn set_start_with_windows(
    #[allow(unused_variables)] app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    value: bool,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        if value {
            manager.enable().map_err(|e| format!("autostart enable: {e}"))?;
        } else {
            manager.disable().map_err(|e| format!("autostart disable: {e}"))?;
        }
    }
    state
        .settings
        .set_start_with_windows(value)
        .map_err(|e| format!("{e:#}"))?;
    Ok(())
}

#[tauri::command]
fn set_close_to_tray(
    state: tauri::State<AppState>,
    value: bool,
) -> Result<(), String> {
    state
        .settings
        .set_close_to_tray(value)
        .map_err(|e| format!("{e:#}"))
}

/// Build the JSON payload we encode into the QR. Kept minimal so the
/// QR can stay scannable at small sizes:
///   { v: 1, fp: "<full fingerprint>", alias, hex, ip, port }
fn build_pair_payload(state: &AppState) -> String {
    let id = &state.identity;
    serde_json::json!({
        "v": 1,
        "type": "millennium-pair",
        "fp": id.fingerprint,
        "alias": id.alias,
        "hex": id.hex_id,
        // Live IP (updated on a network roam), not the boot-time one, so a
        // freshly scanned QR points the phone at the address we're on now.
        "ip": state.discovery.current_local_ip(),
        "port": state.server_port,
    })
    .to_string()
}

#[tauri::command]
fn generate_pair_qr(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let payload = build_pair_payload(&state);
    let code = QrCode::new(payload.as_bytes()).map_err(|e| format!("qr build: {e}"))?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(svg::Color("#00f0ff"))
        .light_color(svg::Color("#050a14"))
        .quiet_zone(true)
        .build();
    Ok(serde_json::json!({
        "svg": svg,
        "payload": build_pair_payload(&state),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairPayload {
    #[serde(default)]
    v: u32,
    #[serde(default, rename = "type")]
    msg_type: String,
    fp: String,
    #[serde(default)]
    alias: String,
    #[serde(default)]
    hex: String,
    ip: String,
    port: u16,
}

#[tauri::command]
async fn pair_with_qr_payload(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    payload: String,
) -> Result<String, String> {
    let parsed: PairPayload = serde_json::from_str(payload.trim())
        .map_err(|e| format!("not a valid Millennium QR payload: {e}"))?;
    if parsed.msg_type != "millennium-pair" || parsed.v != 1 {
        return Err("QR doesn't look like a Millennium pair payload".into());
    }
    if parsed.fp == state.identity.fingerprint {
        return Err("That's our own QR — scan one from another peer.".into());
    }

    // Reuse the manual-peer path: probe first, then save with the real
    // fingerprint returned by /info. This mirrors add_peer_by_ip.
    let info = http_client::fetch_info(&parsed.ip, parsed.port)
        .await
        .map_err(|e| format!("probe {}:{}: {e:#}", parsed.ip, parsed.port))?;
    if info.fingerprint != parsed.fp {
        return Err(format!(
            "QR claimed fp {} but {}:{} answered with {} — refusing to pair",
            &parsed.fp[..16.min(parsed.fp.len())],
            parsed.ip,
            parsed.port,
            &info.fingerprint[..16.min(info.fingerprint.len())]
        ));
    }

    let manual = manual_peers::ManualPeer {
        fingerprint: info.fingerprint.clone(),
        alias: if parsed.alias.is_empty() { info.alias.clone() } else { parsed.alias.clone() },
        hex_id: if parsed.hex.is_empty() { info.hex_id.clone() } else { parsed.hex.clone() },
        icon_type: "desktop".to_string(),
        ip: parsed.ip.clone(),
        port: parsed.port,
    };
    state
        .manual
        .add(manual.clone())
        .map_err(|e| format!("save manual: {e:#}"))?;

    // Also mark favorite so the new peer is immediately visible in the
    // default FAVORITES filter.
    let fav = preferences::FavoritePeer {
        fingerprint: info.fingerprint.clone(),
        alias: manual.alias.clone(),
        hex_id: manual.hex_id.clone(),
        icon_type: manual.icon_type.clone(),
        last_ip: parsed.ip.clone(),
        last_port: parsed.port,
    };
    let _ = state.prefs.add_favorite(fav);

    state.discovery.emit_snapshot(&app);
    Ok(format!("Paired with {} ({}:{})", manual.alias, parsed.ip, parsed.port))
}


// ---------------------------------------------------------------------------
// Manual peers (Fase 8) — register a peer by IP for networks where mDNS is
// blocked (AP isolation, corporate VLANs).
// ---------------------------------------------------------------------------

#[tauri::command]
async fn add_peer_by_ip(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    ip: String,
    port: Option<u16>,
) -> Result<manual_peers::ManualPeer, String> {
    let port = port.unwrap_or(discovery::DEFAULT_PORT);
    let info = http_client::fetch_info(&ip, port)
        .await
        .map_err(|e| format!("could not reach {}:{} — {e:#}", ip, port))?;

    if info.fingerprint == state.identity.fingerprint {
        return Err("that's your own machine".into());
    }

    let icon_type = if info.protocol.contains("phone") || info.protocol.contains("mobile") {
        "phone".to_string()
    } else {
        "desktop".to_string()
    };
    let peer = manual_peers::ManualPeer {
        fingerprint: info.fingerprint.clone(),
        alias: info.alias,
        hex_id: info.hex_id,
        icon_type,
        ip,
        port,
    };
    state.manual.add(peer.clone()).map_err(|e| format!("{e:#}"))?;
    state.discovery.emit_snapshot(&app);
    println!(
        "[backend] add_peer_by_ip → {} {}:{}",
        peer.alias, peer.ip, peer.port
    );
    Ok(peer)
}

#[tauri::command]
fn remove_manual_peer(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
) -> Result<(), String> {
    state.manual.remove(&peer_id).map_err(|e| format!("{e:#}"))?;
    // Also drop it from the live peer cache so the offline ghost
    // disappears without waiting for the reaper.
    state.discovery.peers.lock().unwrap().remove(&peer_id);
    state.discovery.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
fn rename_peer(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
    new_name: String,
) -> Result<(), String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        state.aliases.clear(&peer_id).map_err(|e| format!("{e:#}"))?;
    } else {
        state
            .aliases
            .set(peer_id, trimmed.to_string())
            .map_err(|e| format!("{e:#}"))?;
    }
    state.discovery.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
fn set_peer_icon(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
    icon: String,
) -> Result<(), String> {
    if icon.trim().is_empty() {
        state.icons.clear(&peer_id).map_err(|e| format!("{e:#}"))?;
    } else {
        state
            .icons
            .set(peer_id, icon.trim().to_string())
            .map_err(|e| format!("{e:#}"))?;
    }
    state.discovery.emit_snapshot(&app);
    Ok(())
}

/// Wipe every trace of a peer from local state: live cache, manual
/// entry, favorite flag, alias override, icon override, clipboard-sync
/// setting. The peer will reappear in ALL if mDNS/UDP see it again,
/// but with default name, default icon, no flags.
#[tauri::command]
fn forget_peer(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
) -> Result<(), String> {
    runtime_log::info(format!(
        "[forget] wiping all state for peer {}",
        &peer_id[..16.min(peer_id.len())]
    ));
    state.discovery.peers.lock().unwrap().remove(&peer_id);
    let _ = state.manual.remove(&peer_id);
    let _ = state.prefs.remove_favorite(&peer_id);
    let _ = state.aliases.clear(&peer_id);
    let _ = state.icons.clear(&peer_id);
    let _ = state.clipboard.set(peer_id.clone(), false);
    state.discovery.emit_snapshot(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Auto-update (v0.5.0 F5)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn check_for_update() -> Result<updater::UpdateInfo, String> {
    updater::check_for_update().await.map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------------------
// Crash logging (v0.8.1)
// ---------------------------------------------------------------------------

fn install_panic_hook() {
    // Resolve %APPDATA%/com.guidocameraeq.millennium/ manually so this
    // hook is installable BEFORE Tauri exists and gives us app.path().
    let data_dir = std::env::var_os("APPDATA")
        .map(|p| std::path::PathBuf::from(p).join("com.guidocameraeq.millennium"));

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Let the default handler run too (writes to stderr if attached).
        original_hook(info);

        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backtrace = std::backtrace::Backtrace::force_capture();
        let entry = format!(
            "=== panic @ {when} (millennium v{}) ===\nlocation: {loc}\nmessage:  {payload}\nbacktrace:\n{backtrace}\n\n",
            env!("CARGO_PKG_VERSION"),
        );
        if let Some(dir) = &data_dir {
            let _ = std::fs::create_dir_all(dir);
            let path = dir.join("crash.log");
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = f.write_all(entry.as_bytes());
            }
        }
    }));
}

// ---------------------------------------------------------------------------
// Clipboard sync (v0.6.0)
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "android"))]
enum ClipSnapshot {
    Text(String),
    Image { png_base64: String },
}

#[cfg(target_os = "android")]
fn spawn_clipboard_poller(
    _peers: discovery::PeerMap,
    _store: Arc<clipboard_sync::ClipboardSyncStore>,
    _my_alias: String,
    _my_fingerprint: String,
) {
    // Android: clipboard polling in background is restricted by the
    // OS since Android 10. We'll wire this up via tauri-plugin-clipboard-manager
    // in a later iteration when the foreground service lands.
    runtime_log::info("[clipboard] poller disabled on Android (handled by foreground service later)");
}

// Windows increments this global u32 on every clipboard change, so
// reading it is the cheapest possible "did anything change?" gate.
// Declared by hand instead of pulling the whole `windows` crate in.
#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
}

#[cfg(not(target_os = "android"))]
fn is_syncable_text(t: &str) -> bool {
    !t.is_empty() && t.len() <= 1_000_000
}

#[cfg(not(target_os = "android"))]
enum PollOutcome {
    /// Fresh, syncable content ready to fan out to peers.
    Send(ClipSnapshot),
    /// The clipboard was read fine but there's nothing to send (echo,
    /// unchanged, oversize, or non-syncable content like copied files).
    Handled,
    /// The clipboard was momentarily locked by another process — leave
    /// the change unconsumed so the next tick retries instead of losing it.
    ReadFailed,
}

/// Read the OS clipboard once and decide what to do. Only touches the
/// clipboard; the caller owns the sequence-number gate and the fan-out.
#[cfg(not(target_os = "android"))]
fn poll_clipboard_once(
    cb: &mut arboard::Clipboard,
    last_text: &mut Option<String>,
    last_image_hash: &mut Option<String>,
    store: &clipboard_sync::ClipboardSyncStore,
) -> PollOutcome {
    let text_res = cb.get_text();
    if let Ok(text) = &text_res {
        if !text.is_empty() {
            if last_text.as_deref() == Some(text.as_str()) {
                return PollOutcome::Handled;
            }
            *last_text = Some(text.clone());
            if !is_syncable_text(text) {
                // BUG FIX: oversize text is dropped explicitly instead of
                // falling through to the image branch.
                runtime_log::warn(format!(
                    "[clipboard] text too large ({} bytes) — skipped",
                    text.len()
                ));
                return PollOutcome::Handled;
            }
            *last_image_hash = None;
            let hash = clipboard_sync::hash_text(text);
            if store.is_recent(&hash) {
                return PollOutcome::Handled;
            }
            store.note_synced(hash);
            return PollOutcome::Send(ClipSnapshot::Text(text.clone()));
        }
    }

    let img_res = cb.get_image();
    let img_occupied = matches!(&img_res, Err(arboard::Error::ClipboardOccupied));
    if let Ok(img) = img_res {
        let w = img.width as u32;
        let h = img.height as u32;
        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return PollOutcome::Handled;
        }
        // GATE 3: hash the raw RGBA — same image as last tick means no
        // PNG encode at all.
        let raw_hash = clipboard_sync::hash_bytes(&img.bytes);
        if last_image_hash.as_deref() == Some(raw_hash.as_str()) {
            return PollOutcome::Handled;
        }
        *last_image_hash = Some(raw_hash);
        *last_text = None;

        let raw: Vec<u8> = img.bytes.into_owned();
        let buf = match image::RgbaImage::from_raw(w, h, raw) {
            Some(b) => b,
            None => return PollOutcome::Handled,
        };
        let mut png_bytes: Vec<u8> = Vec::with_capacity(256 * 1024);
        {
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            if image::DynamicImage::ImageRgba8(buf)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .is_err()
            {
                return PollOutcome::Handled;
            }
        }
        if png_bytes.len() > 32 * 1024 * 1024 {
            return PollOutcome::Handled;
        }
        // The anti-echo store keys off the PNG hash: it must match what
        // the /clipboard/image receive path notes (hash of the PNG
        // bytes). The RGBA hash above is only the local "don't re-encode"
        // gate.
        let png_hash = clipboard_sync::hash_bytes(&png_bytes);
        if store.is_recent(&png_hash) {
            return PollOutcome::Handled;
        }
        store.note_synced(png_hash);
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let png_base64 = B64.encode(&png_bytes);
        return PollOutcome::Send(ClipSnapshot::Image { png_base64 });
    }

    // Neither text nor image gave us content. A transient clipboard lock
    // must be retried (ReadFailed); genuinely non-syncable content
    // (copied files, exotic formats) is just seen and dropped (Handled).
    let text_occupied = matches!(&text_res, Err(arboard::Error::ClipboardOccupied));
    if text_occupied || img_occupied {
        PollOutcome::ReadFailed
    } else {
        PollOutcome::Handled
    }
}

#[cfg(not(target_os = "android"))]
fn spawn_clipboard_poller(
    peers: discovery::PeerMap,
    store: Arc<clipboard_sync::ClipboardSyncStore>,
    my_alias: String,
    my_fingerprint: String,
) {
    // The blocking thread hands over snapshots that already passed every
    // gate (peers enabled, changed, not an echo); the async side only
    // fans them out to peers.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ClipSnapshot>(4);

    // ---- Dedicated thread: owns ONE arboard::Clipboard for its whole
    //      life. arboard handles must not cross an .await, so this stays
    //      a std::thread, never a tokio task.
    std::thread::spawn({
        let store = store.clone();
        move || {
            let mut cb = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    runtime_log::err(format!("[clipboard] arboard init failed: {e}"));
                    return;
                }
            };
            let mut last_text: Option<String> = None;
            let mut last_image_hash: Option<String> = None;
            // Prime the sequence number with whatever is already on the
            // clipboard so content copied BEFORE the app launched isn't
            // sent the moment a peer is enabled.
            #[cfg(target_os = "windows")]
            let mut last_seq: u32 = unsafe { GetClipboardSequenceNumber() };

            loop {
                std::thread::sleep(std::time::Duration::from_millis(1200));

                let has_peers = !store.enabled_snapshot().is_empty();

                // GATE 2 (Windows): clipboard unchanged since last tick →
                // zero reads, zero encodes. Read the seq up front but only
                // CONSUME it once we've actually handled the change (below).
                #[cfg(target_os = "windows")]
                let seq = unsafe { GetClipboardSequenceNumber() };
                #[cfg(target_os = "windows")]
                if seq == last_seq {
                    continue;
                }

                // GATE 1: nobody opted into sync → don't read the
                // clipboard. But mark this change as seen so content
                // copied while sync was OFF isn't retroactively sent when
                // the user later enables a peer.
                if !has_peers {
                    #[cfg(target_os = "windows")]
                    {
                        last_seq = seq;
                    }
                    continue;
                }

                match poll_clipboard_once(&mut cb, &mut last_text, &mut last_image_hash, &store)
                {
                    PollOutcome::Send(snap) => {
                        #[cfg(target_os = "windows")]
                        {
                            last_seq = seq;
                        }
                        if tx.blocking_send(snap).is_err() {
                            return; // runtime gone — app is shutting down
                        }
                    }
                    PollOutcome::Handled => {
                        #[cfg(target_os = "windows")]
                        {
                            last_seq = seq;
                        }
                    }
                    // Transient lock: don't consume the seq — retry next tick.
                    PollOutcome::ReadFailed => {}
                }
            }
        }
    });

    // ---- Async side: fan each snapshot out to the opted-in peers.
    tauri::async_runtime::spawn(async move {
        while let Some(snap) = rx.recv().await {
            let targets: Vec<(String, u16)> = {
                let enabled = store.enabled_snapshot();
                if enabled.is_empty() {
                    Vec::new()
                } else {
                    let p = peers.lock().unwrap();
                    enabled
                        .into_iter()
                        .filter(|fp| fp != &my_fingerprint)
                        .filter_map(|fp| p.get(&fp).map(|r| (r.ip.clone(), r.port)))
                        .collect()
                }
            };
            if targets.is_empty() {
                continue;
            }

            for (ip, port) in targets {
                let alias = my_alias.clone();
                let fp = my_fingerprint.clone();
                match &snap {
                    ClipSnapshot::Text(t) => {
                        let text = t.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) =
                                http_client::post_clipboard(&ip, port, &text, &alias, &fp).await
                            {
                                runtime_log::warn(format!(
                                    "[clipboard] text sync to {}:{} failed: {}",
                                    ip, port, e
                                ));
                            }
                        });
                    }
                    ClipSnapshot::Image { png_base64 } => {
                        let b64 = png_base64.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = http_client::post_clipboard_image(
                                &ip, port, &b64, &alias, &fp,
                            )
                            .await
                            {
                                runtime_log::warn(format!(
                                    "[clipboard] image sync to {}:{} failed: {}",
                                    ip, port, e
                                ));
                            }
                        });
                    }
                }
            }
        }
    });
}

#[tauri::command]
fn set_clipboard_sync(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .clipboard
        .set(peer_id.clone(), enabled)
        .map_err(|e| format!("{e:#}"))?;
    println!("[backend] set_clipboard_sync → peer={} enabled={}", peer_id, enabled);
    state.discovery.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
async fn apply_update(
    app: tauri::AppHandle,
    download_url: String,
    expected_sha256: Option<String>,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        updater::download_and_stage(&download_url, expected_sha256.as_deref())
            .await
            .map_err(|e| format!("{e:#}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        app.exit(0);
        Ok(String::new())
    }
    #[cfg(target_os = "android")]
    {
        // Stage the APK directly into the user's public Downloads
        // folder via MediaStore (tauri-plugin-android-fs). The system
        // package installer can't be reliably launched from Tauri 2
        // on Android — every plugin-opener variant we tried hit the
        // OpenArgs deserialization bug — so we make the manual path
        // a pleasant one: the APK lands in /Downloads/, the user
        // taps it from Files / their notifications, Android shows
        // its standard "Install app?" dialog.
        // First download to a temp blob, then publish via MediaStore.
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("resolve cache dir: {e}"))?;
        let staged = updater::download_and_stage_apk(&download_url, &cache_dir, expected_sha256.as_deref())
            .await
            .map_err(|e| format!("{e:#}"))?;
        let bytes = tokio::fs::read(&staged)
            .await
            .map_err(|e| format!("read staged apk: {e}"))?;
        // Best-effort cleanup of the cache copy (the public URI is
        // what we care about going forward).
        let _ = tokio::fs::remove_file(&staged).await;
        let filename = format!(
            "Millennium Clipboard v{}.apk",
            updater::version_for_filename(&download_url)
        );
        let uri = android_fs_bridge::save_to_public_downloads(
            &app,
            &filename,
            &bytes,
            Some("application/vnd.android.package-archive"),
        )
        .await
        .map_err(|e| format!("publish apk to Downloads: {e}"))?;
        // Return the public URI so the frontend can show it.
        Ok(uri)
    }
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        let _ = (app, download_url);
        Err("auto-update not supported on this platform".to_string())
    }
}

#[tauri::command]
fn get_runtime_log() -> String {
    runtime_log::dump_all()
}

#[tauri::command]
fn clear_runtime_log() {
    runtime_log::clear();
}

#[tauri::command]
fn set_log_panel_open(open: bool) {
    runtime_log::set_panel_open(open);
}

/// Consume the "previous update swap failed" marker written by the update
/// batch (updater.rs) when it couldn't replace the running exe after all
/// retries. Returns the message ONCE, then deletes the marker so it shows
/// exactly one time. The frontend calls this at boot AFTER its UI is ready,
/// so a slow/late webview can never lose the notice to an early emit.
#[tauri::command]
fn take_update_failure() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let marker = std::env::temp_dir().join("millennium-update-failed.txt");
        if marker.exists() {
            let detail = std::fs::read_to_string(&marker).unwrap_or_default();
            let _ = std::fs::remove_file(&marker);
            runtime_log::err(format!(
                "[updater] previous update swap failed: {}",
                detail.trim()
            ));
            return Some(
                "Update failed: the new version could not replace the running app (the file was locked). Please retry the update."
                    .to_string(),
            );
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Resolve a (possibly Android `content://`) path string to something
/// `tokio::fs::File::open` can read. On non-Android targets and on
/// Android paths that already look like filesystem paths, this is a
/// passthrough.
#[tauri::command]
async fn prepare_file_for_send(
    #[allow(unused_variables)] app: tauri::AppHandle,
    path: String,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        if path.starts_with("content://") {
            let resolved = android_fs_bridge::resolve_content_uri(&app, path).await?;
            return Ok(resolved.to_string_lossy().to_string());
        }
    }
    Ok(path)
}

#[tauri::command]
fn record_frontend_log(level: String, msg: String) {
    match level.as_str() {
        "ERR" | "ERROR" | "err" | "error" => runtime_log::err(format!("[ui] {}", msg)),
        "WARN" | "warn" => runtime_log::warn(format!("[ui] {}", msg)),
        _ => runtime_log::info(format!("[ui] {}", msg)),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the panic hook BEFORE anything else so we capture
    // crashes that happen during Tauri's own bootstrap, not just our
    // `setup` callback. Release binaries use windows_subsystem="windows"
    // which swallows stderr — without this hook every panic is invisible.
    install_panic_hook();

    // Best-effort: clean up any millennium-clipboard.exe processes left
    // behind by a previous crashed launch BEFORE tauri tries to bind
    // anything. The single-instance plugin handles the normal "second
    // launch" case; this is for zombies that own the port but no longer
    // respond.
    #[cfg(target_os = "windows")]
    windows_integration::kill_other_millennium_processes();

    let mut builder = tauri::Builder::default();

    // Desktop-only plugins. single-instance and autostart don't have an
    // Android backend, and the tray icon lives in build_tray() further
    // down which is already cfg'd desktop.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let _ = argv;
            }))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--autostart"]),
            ));
    }

    // Mobile-only plugins. barcode-scanner requires a camera and Android
    // permissions; not buildable for desktop targets.
    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    }
    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(tauri_plugin_android_fs::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // 0a. Bind the in-memory runtime log to the AppHandle so each
            //     log line is emitted live to the frontend log panel.
            //     Also bind a file appender at <data_dir>/runtime.log so
            //     logs survive app crashes / restarts.
            runtime_log::bind_app(app.handle().clone());
            if let Ok(data_dir_for_log) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&data_dir_for_log);
                runtime_log::bind_file(&data_dir_for_log);
            }
            runtime_log::info(format!(
                "[boot] Millennium Clipboard v{} starting",
                env!("CARGO_PKG_VERSION")
            ));

            // 0a.1 Register an AppUserModelID in HKCU so Windows accepts
            //      toast notifications from this portable .exe.
            //      Also drop the legacy Send To shortcut from v0.10.0.
            #[cfg(target_os = "windows")]
            {
                let icon_candidate = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()));
                windows_integration::register_aumid_for_notifications(
                    icon_candidate.as_deref(),
                );
                windows_integration::cleanup_legacy_send_to_shortcut();
            }

            // 0a.3 A previous auto-update swap that failed after all retries
            //      leaves a marker (see updater.rs). It is surfaced on demand
            //      by the `take_update_failure` command, which the frontend
            //      calls at boot once its UI is ready — a pull, so the message
            //      can never be lost to an emit the webview wasn't yet
            //      listening for. Nothing to do here.

            // 0a.2 Force the window header icon to use our embedded .ico
            //      so it doesn't fall back to the Tauri default glyph.
            #[cfg(desktop)]
            {
                if let Some(main_win) = app.get_webview_window("main") {
                    let icon = tauri::include_image!("icons/icon.png");
                    let _ = main_win.set_icon(icon);

                    #[cfg(target_os = "windows")]
                    {
                        if let Ok(hwnd) = main_win.hwnd() {
                            windows_integration::apply_window_icon_win32(hwnd.0 as isize);
                        } else {
                            runtime_log::warn("[boot] could not resolve hwnd for main window");
                        }
                    }
                } else {
                    runtime_log::warn("[boot] no webview window 'main' to set icon on");
                }
            }

            // 0a.3 System tray. The window-close handler (see bottom of
            //      this setup) hides the window instead of quitting if
            //      `close_to_tray` is on. The tray menu is the only way
            //      to fully exit when that mode is active.
            #[cfg(desktop)]
            build_tray(app.handle())?;

            // 0a.4 If launched by Windows autostart (--autostart flag),
            //      keep the window hidden so we just sit in the tray.
            let launched_hidden = std::env::args().any(|a| a == "--autostart");
            if launched_hidden {
                if let Some(main_win) = app.get_webview_window("main") {
                    let _ = main_win.hide();
                }
                runtime_log::info("[boot] launched via autostart — window hidden, tray-only");
            }


            // 0b. Logging — enabled only when RUST_LOG is set, so it's
            //     silent by default but can be flipped on for debug.
            let _ = env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or("warn"),
            )
            .try_init();

            // 0c. Install the rustls crypto provider before anything uses TLS.
            let _ = rustls::crypto::ring::default_provider().install_default();

            // 0d. Snapshot every IPv4 NIC so we can tell from the log
            //     whether local_ip resolved to the right one.
            runtime_log::log_network_interfaces();

            // 1. Identity + prefs + settings.
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not get app data dir");
            let identity = identity::Identity::load_or_generate(&data_dir)
                .expect("failed to setup identity");
            let prefs = Arc::new(
                preferences::PreferencesStore::load_or_new(&data_dir)
                    .expect("failed to setup preferences"),
            );
            let manual = Arc::new(
                manual_peers::ManualPeerStore::load_or_new(&data_dir)
                    .expect("failed to setup manual peers"),
            );
            let alias_store = Arc::new(
                aliases::AliasStore::load_or_new(&data_dir)
                    .expect("failed to setup aliases"),
            );
            let clipboard_store = Arc::new(
                clipboard_sync::ClipboardSyncStore::load_or_new(&data_dir)
                    .expect("failed to setup clipboard sync"),
            );

            // Compute a sensible default for incoming files. Avoid the
            // Tauri path API here — calling desktop_dir() inside the
            // setup callback can stall on Windows because the shell
            // known-folder lookup runs before COM is fully ready.
            // Default download dir is platform-specific:
            //   - Windows/Linux/Mac: ~/Desktop (so received files land
            //     where the user can see them).
            //   - Android: app-scoped Download folder. The "public"
            //     `/storage/emulated/0/Download` path looks writable
            //     (Rust's create_dir_all returns Ok if it already
            //     exists) but scoped storage on Android 11+ rejects
            //     the actual file write later, which turned into a
            //     500 at receive time. The app-scoped folder
            //     `Android/data/<pkg>/files/Download` always works,
            //     no permissions needed, and Files app shows it. For
            //     images we additionally publish to Pictures/Millennium
            //     after the transfer completes (see http_server.rs).
            #[cfg(target_os = "android")]
            let default_download = app
                .path()
                .app_local_data_dir()
                .ok()
                .map(|p| p.join("Download"))
                .or_else(|| app.path().app_cache_dir().ok().map(|p| p.join("Download")))
                .unwrap_or_else(std::env::temp_dir);

            // Pre-create + check writability on Android. If the dir the
            // OS handed us isn't actually writable, fall back to a temp
            // dir so we don't blow up at receive time with a 500.
            #[cfg(target_os = "android")]
            let default_download = {
                let _ = std::fs::create_dir_all(&default_download);
                let probe = default_download.join(".write_probe");
                if std::fs::write(&probe, b"ok").is_ok() {
                    let _ = std::fs::remove_file(&probe);
                    default_download
                } else {
                    runtime_log::warn(format!(
                        "[setup] default_download_dir {} not writable, falling back to temp",
                        default_download.display()
                    ));
                    std::env::temp_dir()
                }
            };

            #[cfg(not(target_os = "android"))]
            let default_download = std::env::var_os("USERPROFILE")
                .map(|p| PathBuf::from(p).join("Desktop"))
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Desktop")))
                .unwrap_or_else(std::env::temp_dir);

            runtime_log::info(format!(
                "[setup] default_download_dir = {}",
                default_download.display()
            ));
            let settings_store = Arc::new(
                settings::SettingsStore::load_or_default(&data_dir, default_download.clone())
                    .expect("failed to setup settings"),
            );
            runtime_log::info("[setup] settings loaded");

            // 0a.5 Heal stale autostart: the HKCU\...\Run entry keeps the
            //      exe path from whenever enable() last ran, so after an
            //      update it can point at a binary that no longer exists.
            //      With the pref on, disable()+enable() rewrites it to
            //      current_exe(); with the pref off, drop any ghost entry
            //      a previous version left behind.
            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::ManagerExt;
                if settings_store.loaded_from_corrupt() {
                    // settings.json didn't parse → start_with_windows is a
                    // fallback default, not the user's choice. Touching the
                    // Run entry now could wipe a working autostart, so skip.
                    runtime_log::warn(
                        "[autostart] settings unreadable — skipping heal to avoid clobbering the Run entry",
                    );
                } else {
                    let want_autostart = settings_store.snapshot().start_with_windows;
                    let manager = app.autolaunch();
                    if want_autostart {
                        let _ = manager.disable(); // best-effort: may not exist yet
                        if let Err(e) = manager.enable() {
                            runtime_log::err(format!("[autostart] re-register failed: {e}"));
                        } else {
                            runtime_log::info("[autostart] re-registered to current exe path");
                        }
                    } else if let Ok(true) = manager.is_enabled() {
                        let _ = manager.disable();
                        runtime_log::info("[autostart] removed stale entry (pref is off)");
                    }
                }
            }

            // Android migration: if the persisted download_dir (from
            // v0.13.0 or earlier) points at a path that's no longer
            // writable (e.g. /storage/emulated/0/Download/Millennium
            // under scoped storage), reset it to the safe default so
            // file receives don't fail with 500 at write time.
            #[cfg(target_os = "android")]
            {
                let current = settings_store.snapshot().download_dir;
                let _ = std::fs::create_dir_all(&current);
                let probe = current.join(".write_probe");
                if std::fs::write(&probe, b"ok").is_err() {
                    runtime_log::warn(format!(
                        "[setup] persisted download_dir {} not writable, reverting to {}",
                        current.display(),
                        default_download.display()
                    ));
                    let _ = settings_store.set_download_dir(default_download.clone());
                } else {
                    let _ = std::fs::remove_file(&probe);
                }
            }

            let icon_store = Arc::new(
                icon_overrides::IconOverrideStore::load_or_new(&data_dir)
                    .expect("failed to setup icon overrides"),
            );

            // Identity / network / store diagnostic dump.
            runtime_log::info(format!(
                "[diag] identity fp={} alias='{}' local_ip={}",
                &identity.fingerprint[..16.min(identity.fingerprint.len())],
                identity.alias,
                identity.local_ip
            ));
            let manual_snap = manual.snapshot();
            runtime_log::info(format!(
                "[diag] manual-peers count = {}",
                manual_snap.len()
            ));
            for m in &manual_snap {
                runtime_log::info(format!(
                    "[diag]   manual: fp={} alias='{}' {}:{}",
                    &m.fingerprint[..16.min(m.fingerprint.len())],
                    m.alias,
                    m.ip,
                    m.port
                ));
            }

            // 2. HTTPS server.
            let info = http_server::InfoResponse {
                alias: identity.alias.clone(),
                fingerprint: identity.fingerprint.clone(),
                hex_id: identity.hex_id.clone(),
                version: env!("CARGO_PKG_VERSION").into(),
                protocol: "millennium/1".into(),
            };
            let cert_pem = identity.cert_pem.clone();
            let key_pem = identity.key_pem.clone();
            let server_app = app.handle().clone();
            let prefs_for_server = prefs.clone();
            let settings_for_server = settings_store.clone();
            let clipboard_for_server = clipboard_store.clone();

            // Port auto-fallback. If 53319 is already taken (another
            // instance, dev double-launch, OneDrive sync zombie) we try
            // 53320..53328. mDNS/UDP carry the actual port in their
            // payloads, so picking a different one is transparent to
            // remote peers.
            let requested_port = discovery::local_port();

            // The single-instance plugin + the zombie-kill we ran above
            // before Tauri started should mean port 53319 is free now.
            // Fallback below still tries 53319..53328 just in case.

            let server_port = http_server::find_free_tcp_port(requested_port, 10)
                .unwrap_or_else(|| {
                    runtime_log::err(format!(
                        "[setup] no free TCP port found in {}..{} — bind WILL fail",
                        requested_port,
                        requested_port + 10
                    ));
                    let _ = app.handle().emit(
                        "backend-error",
                        format!(
                            "No free TCP port in {}..{}. Close other Millennium instances and reopen.",
                            requested_port,
                            requested_port + 9
                        ),
                    );
                    requested_port
                });
            if server_port != requested_port {
                runtime_log::warn(format!(
                    "[setup] port {} was taken — using {} instead",
                    requested_port, server_port
                ));
            }
            runtime_log::info(format!(
                "[setup] spawning HTTPS server on 0.0.0.0:{}",
                server_port
            ));
            tauri::async_runtime::spawn(async move {
                let err_handle = server_app.clone();
                if let Err(e) = http_server::run(
                    server_app,
                    server_port,
                    info,
                    cert_pem,
                    key_pem,
                    prefs_for_server,
                    settings_for_server,
                    clipboard_for_server,
                )
                .await
                {
                    runtime_log::err(format!("[http] server error: {e:?}"));
                    let _ = err_handle.emit("backend-error", format!("HTTPS server failed: {e}"));
                }
            });

            // Self-ping the HTTPS server after a brief delay to confirm
            // it bound successfully. If this fails the user knows port
            // 53319 is unusable on this machine without having to read
            // stderr.
            let selfping_port = server_port;
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    crate::http_client::fetch_info("127.0.0.1", selfping_port),
                )
                .await
                {
                    Ok(Ok(info)) => runtime_log::info(format!(
                        "[selfping] OK — local /info responded fp={} alias='{}'",
                        &info.fingerprint[..16.min(info.fingerprint.len())],
                        info.alias
                    )),
                    Ok(Err(e)) => runtime_log::err(format!(
                        "[selfping] FAILED — local /info errored: {e:?}"
                    )),
                    Err(_) => runtime_log::err(
                        "[selfping] FAILED — local /info timed out (server didn't bind?)",
                    ),
                }
            });

            // 3. mDNS discovery — announces with the *real* tcp port
            //    we ended up bound to.
            runtime_log::info("[setup] starting mDNS discovery...");
            let handle = app.handle().clone();
            let discovery_state = discovery::start(
                handle,
                &identity,
                server_port,
                prefs.clone(),
                manual.clone(),
                alias_store.clone(),
                clipboard_store.clone(),
                icon_store.clone(),
            )
            .expect("failed to start mDNS discovery");
            runtime_log::info("[setup] mDNS discovery started");

            // 4. UDP broadcast discovery — also carries the real tcp port.
            let udp_info = udp_discovery::LocalInfo {
                alias: identity.alias.clone(),
                fingerprint: identity.fingerprint.clone(),
                hex_id: identity.hex_id.clone(),
                tcp_port: server_port,
                local_ip: identity.local_ip.clone(),
            };
            udp_discovery::spawn(
                app.handle().clone(),
                udp_info,
                discovery_state.peers.clone(),
                prefs.clone(),
                manual.clone(),
                alias_store.clone(),
                clipboard_store.clone(),
                icon_store.clone(),
            );

            // 5. Clipboard-sync poller. Watches the OS clipboard on a
            //    dedicated thread (gated: only when peers opted in and
            //    the clipboard actually changed) and broadcasts to them.
            spawn_clipboard_poller(
                discovery_state.peers.clone(),
                clipboard_store.clone(),
                identity.alias.clone(),
                identity.fingerprint.clone(),
            );

            app.manage(AppState {
                discovery: discovery_state,
                identity,
                prefs,
                settings: settings_store,
                manual,
                aliases: alias_store,
                clipboard: clipboard_store,
                icons: icon_store,
                server_port,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_local_info,
            list_peers,
            rescan_peers,
            send_text,
            send_files,
            approve_session,
            reject_session,
            cancel_session,
            toggle_favorite,
            get_settings,
            set_download_dir,
            set_auto_accept_favorites,
            add_peer_by_ip,
            remove_manual_peer,
            rename_peer,
            check_for_update,
            apply_update,
            set_clipboard_sync,
            get_runtime_log,
            clear_runtime_log,
            set_log_panel_open,
            take_update_failure,
            record_frontend_log,
            set_peer_icon,
            forget_peer,
            set_notifications_enabled,
            set_start_with_windows,
            set_close_to_tray,
            generate_pair_qr,
            pair_with_qr_payload,
            prepare_file_for_send,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Read the current close_to_tray setting; if ON, hide
                // the window and keep the process alive in the tray.
                let state = window.app_handle().state::<AppState>();
                if state.settings.snapshot().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(desktop)]
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

    let open_i = MenuItem::with_id(app, "tray_open", "Open Millennium", true, None::<&str>)?;
    let send_i = MenuItem::with_id(app, "tray_send", "Send to peer…", true, None::<&str>)?;
    let clip_i = MenuItem::with_id(
        app,
        "tray_clip_toggle",
        "Toggle clipboard sync (all)",
        true,
        None::<&str>,
    )?;
    let log_i = MenuItem::with_id(app, "tray_log", "Open log", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &send_i, &clip_i, &log_i, &sep, &quit_i])?;

    let icon = tauri::include_image!("icons/icon.ico");

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(false)
        .tooltip("Millennium Clipboard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_open" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "tray_send" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                    let _ = w.emit("tray-action", "send");
                }
            }
            "tray_clip_toggle" => {
                let _ = app.emit("tray-action", "toggle-clipboard");
            }
            "tray_log" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                    let _ = w.emit("tray-action", "log");
                }
            }
            "tray_quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Double-click (left button, "up" twice in a row counts as
            // DoubleClick on most platforms — Tauri's TrayIconEvent
            // surfaces it directly as DoubleClick).
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                if let Some(w) = tray.app_handle().get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversize_text() {
        assert!(!is_syncable_text(&"x".repeat(1_000_001)));
        assert!(is_syncable_text("hola"));
        assert!(!is_syncable_text(""));
    }
}
