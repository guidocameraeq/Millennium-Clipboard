// Tipos crudos de la CCD API. Migrado de Monarch @ 7f9f63b
// (`src-tauri/src/backend/windows/win32_types.rs`) — ver docs/DECISIONS.md ADR-002.
//
// El archivo entero es windows-only: el atributo interno de abajo lo apaga en
// cualquier otro target, igual que `windows_integration.rs`.
//
// FASE 2: se restauró lo que la Fase 1 había podado — `AttachablePath` y el
// campo `attachable`. Son los candidatos de re-adjuntado y solo tienen sentido
// cuando existe un `SetDisplayConfig` que los use, que es lo que llegó ahora.
#![cfg(target_os = "windows")]

use monarch::{DisplayId, DisplayInfo, Layout};
use windows::Win32::Devices::Display::{DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO};

#[derive(Clone)]
pub struct RawTopologySnapshot {
    pub paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    pub modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}

/// Un path de un target conectado-pero-inactivo, cosechado de `QDC_ALL_PATHS`.
///
/// Queda **fuera** de `RawTopologySnapshot::paths` a propósito:
/// `apply_layout_against_snapshot` le pasa `raw.paths` derecho a
/// `SetDisplayConfig`, y estos paths son **candidatos**, no la configuración
/// actual. Mezclarlos activaría monitores que nadie pidió.
///
/// `ALL_PATHS` reporta una entrada por combinación (source, target) y se
/// conservan **todas**: el source recién se elige en el momento del attach.
///
/// Esta es la primera cicatriz de la doctrina CCD (Monarch ADR-003): el path de
/// la TV ya venía enumerado y el código viejo lo tiraba "por prudencia",
/// mientras el rescate le rogaba a Windows un extend que no podía funcionar.
#[derive(Clone)]
pub struct AttachablePath {
    pub path: DISPLAYCONFIG_PATH_INFO,
    pub adapter_luid: u64,
    pub target_id: u32,
}

#[derive(Clone)]
pub struct TopologySnapshot {
    pub raw: RawTopologySnapshot,
    pub layout: Layout,
    pub displays: Vec<DisplayInfo>,
    /// Candidatos de attach para los monitores conectados-pero-apagados. Queda
    /// **vacío** en los snapshots que no vienen de una enumeración `ALL_PATHS`
    /// (por ejemplo el snapshot mínimo que usa un detach).
    pub attachable: Vec<AttachablePath>,
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
