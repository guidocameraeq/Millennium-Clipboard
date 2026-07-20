// Tipos crudos de la CCD API. Migrado de Monarch @ 7f9f63b
// (`src-tauri/src/backend/windows/win32_types.rs`) — ver docs/DECISIONS.md ADR-002.
//
// El archivo entero es windows-only: el atributo interno de abajo lo apaga en
// cualquier otro target, igual que `windows_integration.rs`.
//
// PODADO respecto del donante: no viaja `AttachablePath` ni el campo `attachable`
// de `TopologySnapshot`. Son candidatos de re-adjuntado, material exclusivo de
// `SetDisplayConfig` (Fase 2). La Fase 1 solo mira.
#![cfg(target_os = "windows")]

use monarch::{DisplayId, DisplayInfo, Layout};
use windows::Win32::Devices::Display::{DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO};

#[derive(Clone)]
pub struct RawTopologySnapshot {
    pub paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    pub modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}

#[derive(Clone)]
pub struct TopologySnapshot {
    #[allow(dead_code)] // la geometría cruda recién la usa el apply de la Fase 2
    pub raw: RawTopologySnapshot,
    pub layout: Layout,
    pub displays: Vec<DisplayInfo>,
}

pub fn luid_to_u64(high_part: i32, low_part: u32) -> u64 {
    ((high_part as i64 as u64) << 32) | (low_part as u64)
}

pub fn make_display_id(adapter_luid: u64, target_id: u32, edid_hash: Option<u64>) -> DisplayId {
    DisplayId {
        adapter_luid,
        target_id,
        edid_hash,
    }
}
