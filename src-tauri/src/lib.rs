// Millennium Clipboard — backend (Fase 3)
//
// Peers now come from the real mDNS daemon in `discovery::`. The
// command surface is unchanged; only the implementation moved off mocks.
// Transfer commands still log instead of doing real network I/O — that
// arrives in Fase 5.

use serde::{Deserialize, Serialize};
use tauri::Manager;

mod discovery;

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

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    discovery: discovery::DiscoveryState,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_local_info(state: tauri::State<AppState>) -> LocalInfo {
    let id = &state.discovery.identity;
    LocalInfo {
        alias: id.alias.clone(),
        host_id_hex: id.hex_id.clone(),
        ip: id.local_ip.clone(),
        port: discovery::SERVICE_PORT,
        fingerprint: id.uuid.clone(),
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
fn send_text(peer_id: String, text: String) -> Result<(), String> {
    println!(
        "[backend] send_text → peer={} chars={}",
        peer_id,
        text.chars().count()
    );
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
    // Persistence arrives in Fase 6.
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
            let handle = app.handle().clone();
            let discovery_state = discovery::start(handle)
                .expect("failed to start mDNS discovery");
            app.manage(AppState { discovery: discovery_state });
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
