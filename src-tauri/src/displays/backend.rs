// Selector de backend — Fase 2 del SPEC-displays. Migrado de Monarch @ 7f9f63b
// (`src-tauri/src/backend/mod.rs`) — ver docs/DECISIONS.md ADR-002.
//
// El `MonarchDisplayManager` habla con UN `DisplayBackend`. Acá se decide cuál:
// el de verdad (CCD API sobre Win32) o el de mentira, para poder ensayar el
// auto-rollback sin arriesgar la TV. Este archivo no tiene lógica propia: es
// despacho por `match` y nada más.
//
// Diferencia con el donante: allá el enum tenía `#[cfg(target_os = "windows")]`
// en la variante `Windows` porque el archivo compilaba en todos los targets.
// Acá el archivo entero es windows-only (atributo interno de abajo), así que
// esos `cfg` sobrarían y solo harían ruido.
#![cfg(target_os = "windows")]

use monarch::{
    DisplayBackend, DisplayId, DisplayInfo, Layout, ManagerError, MockBackend, OutputConfig,
    Position, Resolution,
};

use super::topology::WindowsDisplayBackend;

/// El backend que usa el manager: monitores reales o monitores de mentira.
pub enum SystemDisplayBackend {
    Windows(WindowsDisplayBackend),
    Mock(MockBackend),
}

impl SystemDisplayBackend {
    /// Elige backend. Con `MONARCH_FORCE_MOCK_BACKEND` seteada, el falso.
    ///
    /// La env var conserva el nombre de Monarch (está en la constante que ya
    /// usa el camino de lectura en `mod.rs`), así que una sola variable pone en
    /// modo mentira **todo** el módulo: la foto y el apply. Si cada uno tuviera
    /// la suya, el usuario podría terminar ensayando el rollback contra sus
    /// monitores de verdad creyendo que está en la demo.
    pub fn new() -> Result<Self, ManagerError> {
        if std::env::var_os(super::FORCE_MOCK_ENV).is_some() {
            super::diagnostics::log(format!(
                "{} activo — el apply corre contra monitores FALSOS, nada toca el hardware",
                super::FORCE_MOCK_ENV
            ));
            return Ok(Self::Mock(build_mock_backend()?));
        }
        Ok(Self::Windows(WindowsDisplayBackend::new()?))
    }
}

impl DisplayBackend for SystemDisplayBackend {
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ManagerError> {
        match self {
            Self::Windows(backend) => backend.list_displays(),
            Self::Mock(backend) => backend.list_displays(),
        }
    }

    fn get_layout(&self) -> Result<Layout, ManagerError> {
        match self {
            Self::Windows(backend) => backend.get_layout(),
            Self::Mock(backend) => backend.get_layout(),
        }
    }

    fn apply_layout(&self, layout: Layout) -> Result<(), ManagerError> {
        match self {
            Self::Windows(backend) => backend.apply_layout(layout),
            Self::Mock(backend) => backend.apply_layout(layout),
        }
    }

    fn color_state_signature(&self) -> Result<Option<String>, ManagerError> {
        match self {
            Self::Windows(backend) => backend.color_state_signature(),
            Self::Mock(backend) => backend.color_state_signature(),
        }
    }

    fn reapply_color_calibration(&self) -> Result<(), ManagerError> {
        match self {
            Self::Windows(backend) => backend.reapply_color_calibration(),
            Self::Mock(backend) => backend.reapply_color_calibration(),
        }
    }

    fn invalidate_cache(&self) -> Result<(), ManagerError> {
        match self {
            Self::Windows(backend) => backend.invalidate_cache(),
            // `MockBackend` no tiene cache propia: se llama la implementación
            // por defecto del trait, escrita así (forma explícita, como en el
            // donante) para que quede claro que no hay método inherente que
            // pudiera ganarle a la resolución de nombres.
            Self::Mock(backend) => DisplayBackend::invalidate_cache(backend),
        }
    }

    fn prepare_attach_targets(&self, desired: &Layout) -> Result<(), ManagerError> {
        match self {
            Self::Windows(backend) => backend.prepare_attach_targets(desired),
            Self::Mock(backend) => DisplayBackend::prepare_attach_targets(backend, desired),
        }
    }
}

/// Los tres monitores de mentira.
///
/// Son los mismos que devuelve `mock_displays()` en `mod.rs` —mismos ids
/// (`1:1:1`, `1:2:2`, `1:3:3`), mismos nombres, mismas posiciones— y eso NO es
/// casualidad: el usuario va a ensayar el auto-rollback en modo mock, y si la
/// lista que ve no coincidiera con la que el apply modifica, el ensayo no
/// probaría nada.
///
/// Única diferencia, deliberada: el vertical desconectado acá tiene un modo de
/// verdad (1080x1920) mientras que la vista de `mod.rs` lo muestra en 0x0. Así
/// se comporta el hardware real —un monitor detachado no reporta modo activo,
/// y al re-adjuntarlo Windows le da uno—, así que al ensayar el attach la fila
/// pasa de "—" a 1080x1920 igual que pasaría con la TV. Ponerle 0x0 al backend
/// haría que el attach del ensayo *parezca* fallado.
fn build_mock_backend() -> Result<MockBackend, ManagerError> {
    let primary = DisplayInfo {
        id: DisplayId {
            adapter_luid: 1,
            target_id: 1,
            edid_hash: Some(1),
        },
        friendly_name: "Primary Panel (Mock)".to_string(),
        is_active: true,
        is_primary: true,
        resolution: Resolution {
            width: 1920,
            height: 1080,
        },
        refresh_rate_mhz: 60_000,
    };
    let side = DisplayInfo {
        id: DisplayId {
            adapter_luid: 1,
            target_id: 2,
            edid_hash: Some(2),
        },
        friendly_name: "Side Display (Mock)".to_string(),
        is_active: true,
        is_primary: false,
        resolution: Resolution {
            width: 2560,
            height: 1440,
        },
        refresh_rate_mhz: 144_000,
    };
    let portrait = DisplayInfo {
        id: DisplayId {
            adapter_luid: 1,
            target_id: 3,
            edid_hash: Some(3),
        },
        friendly_name: "Portrait Display (Mock)".to_string(),
        is_active: false,
        is_primary: false,
        resolution: Resolution {
            width: 1080,
            height: 1920,
        },
        refresh_rate_mhz: 60_000,
    };

    // Los outputs se arman desde los `DisplayInfo` de arriba (clonando id,
    // resolución y refresco) en vez de indexar un vector: `MockBackend::new`
    // después sincroniza los displays contra este layout, así que cualquier
    // desprolijidad acá se propaga a lo que ve la UI.
    let layout = Layout {
        outputs: vec![
            OutputConfig {
                display_id: primary.id.clone(),
                enabled: true,
                position: Position { x: 0, y: 0 },
                resolution: primary.resolution.clone(),
                refresh_rate_mhz: primary.refresh_rate_mhz,
                primary: true,
            },
            OutputConfig {
                display_id: side.id.clone(),
                enabled: true,
                position: Position { x: 1920, y: 0 },
                resolution: side.resolution.clone(),
                refresh_rate_mhz: side.refresh_rate_mhz,
                primary: false,
            },
            OutputConfig {
                display_id: portrait.id.clone(),
                // Desconectado a propósito: es el que permite ensayar el
                // attach (y su rollback) sin desenchufar nada.
                enabled: false,
                position: Position { x: -1080, y: 0 },
                resolution: portrait.resolution.clone(),
                refresh_rate_mhz: portrait.refresh_rate_mhz,
                primary: false,
            },
        ],
    };

    MockBackend::new(vec![primary, side, portrait], layout)
}
