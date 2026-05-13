// Millennium Clipboard — backend (Fase 4)
//
// Setup wires three subsystems:
//   1. Identity (cert + fingerprint, persisted)
//   2. HTTPS server (exposes /info for peer cross-check)
//   3. mDNS discovery (announces our service, lists peers)
//
// Transfer commands still log instead of doing network I/O — Fase 5.

use serde::{Deserialize, Serialize};
use tauri::Manager;

mod discovery;
mod http_client;
mod http_server;
mod identity;

// ---------------------------------------------------------------------------
// Wire types
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

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    discovery: discovery::DiscoveryState,
    identity: identity::Identity,
}

// ---------------------------------------------------------------------------
// Commands
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
    state
        .discovery
        .peers
        .lock()
        .unwrap()
        .values()
        .map(|r| r.to_wire(false))
        .collect()
}

#[tauri::command]
fn rescan_peers(state: tauri::State<AppState>) -> Result<Vec<discovery::WirePeer>, String> {
    discovery::rebrowse(&state.discovery).map_err(|e| e.to_string())?;
    Ok(state
        .discovery
        .peers
        .lock()
        .unwrap()
        .values()
        .map(|r| r.to_wire(false))
        .collect())
}

#[tauri::command]
async fn send_text(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    text: String,
) -> Result<(), String> {
    // 1. Look the peer up in the discovery cache.
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

    // 2. Cross-check identity: what the server reports must match the
    //    fingerprint mDNS advertised. Mismatch = MITM or stale cache.
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

    // 3. Send the text.
    http_client::post_text(
        &target.ip,
        target.port,
        text,
        state.identity.alias.clone(),
        state.identity.fingerprint.clone(),
    )
    .await
    .map_err(|e| format!("send failed: {e:#}"))?;

    Ok(())
}

#[tauri::command]
fn send_files(peer_id: String, file_paths: Vec<String>) -> Result<(), String> {
    println!(
        "[backend] send_files → peer={} files={}",
        peer_id,
        file_paths.len()
    );
    Ok(())
}

#[tauri::command]
fn toggle_favorite(peer_id: String, value: bool) -> Result<(), String> {
    println!("[backend] toggle_favorite → peer={} value={}", peer_id, value);
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 0. Pick a TLS crypto provider before anything uses rustls
            //    (axum-server + reqwest both auto-select otherwise).
            let _ = rustls::crypto::ring::default_provider().install_default();

            // 1. Identity (load or generate cert)
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not get app data dir");
            let identity = identity::Identity::load_or_generate(&data_dir)
                .expect("failed to setup identity");

            // 2. HTTPS server (background task)
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
            tauri::async_runtime::spawn(async move {
                if let Err(e) = http_server::run(
                    server_app,
                    discovery::local_port(),
                    info,
                    cert_pem,
                    key_pem,
                )
                .await
                {
                    eprintln!("[http] server error: {e:?}");
                }
            });

            // 3. mDNS discovery
            let handle = app.handle().clone();
            let discovery_state = discovery::start(handle, &identity, discovery::local_port())
                .expect("failed to start mDNS discovery");

            app.manage(AppState {
                discovery: discovery_state,
                identity,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_local_info,
            list_peers,
            rescan_peers,
            send_text,
            send_files,
            toggle_favorite,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
