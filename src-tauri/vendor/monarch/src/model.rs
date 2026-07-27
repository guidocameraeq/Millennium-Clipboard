use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ManagerError;

pub const DEFAULT_PROFILE_SHORTCUT_BASE: &str = "Ctrl+Shift";
pub const DEFAULT_DISPLAY_TOGGLE_SHORTCUT_BASE: &str = "Ctrl+Alt";

fn default_global_shortcuts_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DisplayId {
    pub adapter_luid: u64,
    pub target_id: u32,
    pub edid_hash: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub friendly_name: String,
    pub is_active: bool,
    pub is_primary: bool,
    pub resolution: Resolution,
    pub refresh_rate_mhz: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub display_id: DisplayId,
    pub enabled: bool,
    pub position: Position,
    pub resolution: Resolution,
    pub refresh_rate_mhz: u32,
    pub primary: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub outputs: Vec<OutputConfig>,
}

impl Layout {
    pub fn enabled_output_count(&self) -> usize {
        self.outputs.iter().filter(|output| output.enabled).count()
    }

    pub fn ensure_valid(&self) -> Result<(), ManagerError> {
        if self.outputs.is_empty() {
            return Err(ManagerError::Validation(
                "layout cannot be empty".to_string(),
            ));
        }

        if self.enabled_output_count() == 0 {
            return Err(ManagerError::Validation(
                "layout must have at least one enabled display".to_string(),
            ));
        }

        Ok(())
    }

    pub fn find_output_index(&self, display_id: &DisplayId) -> Option<usize> {
        self.outputs
            .iter()
            .position(|output| &output.display_id == display_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub layout: Layout,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayFingerprint {
    pub display_id: DisplayId,
    pub friendly_name: String,
    pub edid_fingerprint: Option<String>,
}

/// Salida de audio deseada por un perfil (Displays v2, Fase 3).
///
/// `endpoint_id` es la clave técnica opaca del endpoint (de `IMMDevice::GetId`);
/// `friendly_name` es la etiqueta linda para la UI y el fallback si el id ya no
/// resuelve (cambio de driver, USB a otro puerto). Ambos `String` → `Eq` trivial.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioTarget {
    pub endpoint_id: String,
    pub friendly_name: String,
}

/// Una acción que dispara un perfil-escena (Displays v2, Fase 4).
///
/// Etiquetado por el campo `tipo` (internally tagged) para poder sumar variantes
/// nuevas sin romper los stores viejos. JSON: `{"tipo":"lanzar","destino":"…",
/// "args":["…"]}` o `{"tipo":"volumen","nivel":40}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Accion {
    /// Lanzar un `.exe` (con args EN LISTA, sin shell), una URI de protocolo
    /// (`steam://…`) o un `.lnk`. El dispatch por tipo de destino lo hace el
    /// motor de acciones; acá es solo el dato. `args` es opcional en el JSON.
    Lanzar {
        destino: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// Fijar el volumen (0–100) de la salida de audio por default actual.
    Volumen { nivel: u8 },
}

/// Las acciones de un perfil-escena (Displays v2, Fase 4): las de ENTRADA corren
/// al aplicar/confirmar el perfil; las de SALIDA corren cuando se pasa a OTRO
/// perfil (modelo "ida y vuelta"). Ambas listas vacías = perfil sin acciones,
/// idéntico a como se comportaba antes de la Fase 4.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerfilAcciones {
    pub entrada: Vec<Accion>,
    pub salida: Vec<Accion>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub revert_timeout_secs: u64,
    pub start_with_windows: bool,
    pub startup_profile_name: Option<String>,
    #[serde(default = "default_global_shortcuts_enabled")]
    pub global_shortcuts_enabled: bool,
    pub profile_shortcut_base: Option<String>,
    pub display_toggle_shortcut_base: Option<String>,
    pub profile_shortcuts: BTreeMap<String, String>,
    pub display_toggle_shortcuts: BTreeMap<String, String>,
    /// Salida de audio deseada por perfil (Displays v2, Fase 3), keyed por
    /// **nombre** de perfil — mismo lugar y patrón que `profile_shortcuts`. Sin
    /// entrada = ese perfil está en "No tocar el audio". Nace vacío en stores
    /// viejos gracias al `#[serde(default)]` a nivel struct.
    pub profile_audio: BTreeMap<String, AudioTarget>,
    /// Acciones por perfil-escena (Displays v2, Fase 4), keyed por **nombre** de
    /// perfil — mismo lugar y patrón que `profile_audio`. Sin entrada = perfil
    /// sin acciones (como antes de la Fase 4). Nace vacío en stores viejos por el
    /// `#[serde(default)]` a nivel struct.
    pub profile_actions: BTreeMap<String, PerfilAcciones>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            revert_timeout_secs: 10,
            start_with_windows: false,
            startup_profile_name: None,
            global_shortcuts_enabled: true,
            profile_shortcut_base: Some(DEFAULT_PROFILE_SHORTCUT_BASE.to_string()),
            display_toggle_shortcut_base: Some(DEFAULT_DISPLAY_TOGGLE_SHORTCUT_BASE.to_string()),
            profile_shortcuts: BTreeMap::new(),
            display_toggle_shortcuts: BTreeMap::new(),
            profile_audio: BTreeMap::new(),
            profile_actions: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub profiles: Vec<Profile>,
    pub display_fingerprints: Vec<DisplayFingerprint>,
    pub last_known_good_layout: Option<Layout>,
    pub last_restorable_layout: Option<Layout>,
    pub settings: AppSettings,
}
