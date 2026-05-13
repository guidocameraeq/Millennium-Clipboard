// Millennium Clipboard — backend (Fase 7)
//
// Wires identity, persisted prefs/settings, HTTPS server, mDNS discovery,
// and the HTTPS client used to talk to peers. Commands invoked from JS
// are at the bottom.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use uuid::Uuid;

mod aliases;
mod clipboard_sync;
mod discovery;
mod http_client;
mod http_server;
mod identity;
mod manual_peers;
mod preferences;
mod settings;
mod udp_discovery;
mod updater;

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
        ip: id.local_ip.clone(),
        port: discovery::local_port(),
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
        discovery::local_port(),
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
        prepare_files.push(http_client::PrepareFile {
            file_id: file_id.clone(),
            name: name.clone(),
            size,
            mime,
            sha256: None, // MVP: skip hashing big files; add in Fase 8 polish
            rel_path: None,
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
        discovery::local_port(),
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

fn spawn_clipboard_poller(
    peers: discovery::PeerMap,
    store: Arc<clipboard_sync::ClipboardSyncStore>,
    my_alias: String,
    my_fingerprint: String,
) {
    tauri::async_runtime::spawn(async move {
        let mut last_seen: Option<String> = None;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        tick.tick().await; // skip immediate first tick
        loop {
            tick.tick().await;

            // Reading the OS clipboard is blocking — keep it off the
            // tokio worker pool.
            let text: Option<String> = tokio::task::spawn_blocking(|| {
                arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut cb| cb.get_text().ok())
            })
            .await
            .ok()
            .flatten();

            let Some(text) = text else { continue };
            if text.is_empty() || text.len() > 1_000_000 {
                continue;
            }
            if last_seen.as_deref() == Some(text.as_str()) {
                continue;
            }
            last_seen = Some(text.clone());

            let hash = clipboard_sync::hash_text(&text);
            // Loop prevention: skip if we just applied this hash from a peer.
            if store.is_recent(&hash) {
                continue;
            }
            store.note_synced(hash.clone());

            // Collect enabled+online targets.
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
                let text = text.clone();
                let alias = my_alias.clone();
                let fp = my_fingerprint.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) =
                        http_client::post_clipboard(&ip, port, &text, &alias, &fp).await
                    {
                        eprintln!("[clipboard] sync to {}:{} failed: {}", ip, port, e);
                    }
                });
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
async fn apply_update(app: tauri::AppHandle, download_url: String) -> Result<(), String> {
    updater::download_and_stage(&download_url)
        .await
        .map_err(|e| format!("{e:#}"))?;
    // Give the batch script a chance to start, then exit so it can move
    // the file in place.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.exit(0);
    Ok(())
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 0a. Logging — enabled only when RUST_LOG is set, so it's
            //     silent by default but can be flipped on for debug.
            let _ = env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or("warn"),
            )
            .try_init();

            // 0b. Install the rustls crypto provider before anything uses TLS.
            let _ = rustls::crypto::ring::default_provider().install_default();

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
            let default_download = std::env::var_os("USERPROFILE")
                .map(|p| PathBuf::from(p).join("Desktop"))
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Desktop")))
                .unwrap_or_else(std::env::temp_dir);
            eprintln!("[setup] default_download_dir = {}", default_download.display());
            eprintln!("[setup] loading settings...");
            let settings_store = Arc::new(
                settings::SettingsStore::load_or_default(&data_dir, default_download)
                    .expect("failed to setup settings"),
            );
            eprintln!("[setup] settings loaded");

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
            eprintln!("[setup] spawning HTTPS server task...");
            tauri::async_runtime::spawn(async move {
                eprintln!("[setup] http_server::run starting");
                if let Err(e) = http_server::run(
                    server_app,
                    discovery::local_port(),
                    info,
                    cert_pem,
                    key_pem,
                    prefs_for_server,
                    settings_for_server,
                    clipboard_for_server,
                )
                .await
                {
                    eprintln!("[http] server error: {e:?}");
                }
            });
            eprintln!("[setup] HTTPS server spawned");

            // 3. mDNS discovery.
            eprintln!("[setup] starting discovery...");
            let handle = app.handle().clone();
            let discovery_state = discovery::start(
                handle,
                &identity,
                discovery::local_port(),
                prefs.clone(),
                manual.clone(),
                alias_store.clone(),
                clipboard_store.clone(),
            )
            .expect("failed to start mDNS discovery");
            eprintln!("[setup] discovery started");

            // 4. UDP broadcast discovery — runs alongside mDNS so peers
            //    appear even on networks that filter multicast.
            let udp_info = udp_discovery::LocalInfo {
                alias: identity.alias.clone(),
                fingerprint: identity.fingerprint.clone(),
                hex_id: identity.hex_id.clone(),
                tcp_port: discovery::local_port(),
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
            );

            // 5. Clipboard-sync poller. Reads the OS clipboard every
            //    500 ms and broadcasts changes to opted-in peers.
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
