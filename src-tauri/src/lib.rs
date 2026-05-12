// Millennium Clipboard — backend
// Fase 2: command surface defined; data is still mocked. Fase 3+ replaces
// the mocks with real mDNS discovery and HTTPS transport.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wire types — serialized as camelCase to match the JS frontend conventions.
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerStatus {
    Online,
    Away,
    Offline,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconType {
    Desktop,
    Phone,
    Tablet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String, // fingerprint, stable across sessions
    pub name: String,
    pub hex_id: String, // e.g. "0x4F:A2:E1"
    pub ip: String,
    pub port: u16,
    pub status: PeerStatus,
    pub favorite: bool,
    pub icon_type: IconType,
}

// ---------------------------------------------------------------------------
// Mock data — replaced in Fase 3.
// ---------------------------------------------------------------------------

fn mock_peers() -> Vec<Peer> {
    vec![
        Peer {
            id: "fp-toby".into(),
            name: "TOBY-NOTEBOOK".into(),
            hex_id: "0x4F:A2:E1".into(),
            ip: "192.168.1.43".into(),
            port: 53319,
            status: PeerStatus::Online,
            favorite: true,
            icon_type: IconType::Desktop,
        },
        Peer {
            id: "fp-galaxy".into(),
            name: "GALAXY-S22".into(),
            hex_id: "0x9C:1B:F0".into(),
            ip: "192.168.1.57".into(),
            port: 53319,
            status: PeerStatus::Online,
            favorite: true,
            icon_type: IconType::Phone,
        },
        Peer {
            id: "fp-office".into(),
            name: "OFFICE-DESKTOP".into(),
            hex_id: "0x2A:88:7D".into(),
            ip: "192.168.1.71".into(),
            port: 53319,
            status: PeerStatus::Online,
            favorite: false,
            icon_type: IconType::Desktop,
        },
        Peer {
            id: "fp-ipad".into(),
            name: "IPAD-LUCIA".into(),
            hex_id: "0xE3:55:12".into(),
            ip: "192.168.1.88".into(),
            port: 53319,
            status: PeerStatus::Away,
            favorite: false,
            icon_type: IconType::Tablet,
        },
    ]
}

fn mock_local_info() -> LocalInfo {
    LocalInfo {
        alias: "TOBY-WS".into(),
        host_id_hex: "0x42:7A:9F".into(),
        ip: "192.168.1.42".into(),
        port: 53319,
        fingerprint: "AB:CD:EF:01:23:45:67:89:AB:CD".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands — the public surface invoked from JS via window.__TAURI__.
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_local_info() -> LocalInfo {
    mock_local_info()
}

#[tauri::command]
fn list_peers() -> Vec<Peer> {
    mock_peers()
}

#[tauri::command]
fn rescan_peers() -> Vec<Peer> {
    // In Fase 3 this triggers a mDNS re-query. For now it just returns the
    // same mocks so the SCAN button gives the user feedback.
    println!("[backend] rescan_peers invoked");
    mock_peers()
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
    println!("[backend] toggle_favorite → peer={} value={}", peer_id, value);
    Ok(())
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
