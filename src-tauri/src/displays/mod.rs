//! Módulo de displays — **Fase 1 del SPEC-displays: SOLO LECTURA.**
//!
//! Enumera los monitores conectados (activos y desconectados-pero-presentes) y
//! los devuelve al frontend. Nada de acá cambia la configuración de pantallas.
//!
//! Es verificable, no una promesa: buscá el nombre de la API que cambia
//! monitores en `src-tauri/` y vas a encontrar solo comentarios como éste,
//! ninguna llamada. El motor de apply de Monarch (`apply.rs`, 845 líneas) no se
//! copió — no existe en este repo. Attach/detach con red de seguridad es la
//! Fase 2, y recién ahí entra ese archivo.
//!
//! # Forma del módulo
//!
//! - Este `mod.rs` es **ungateado** a propósito: los tipos que cruzan al frontend
//!   y el comando tienen que poder nombrarse en cualquier plataforma. Si el DTO
//!   viviera tras `cfg(windows)`, la firma del comando no compilaría en Android.
//! - `enumerate` y `win32_types` son **windows-only**, con doble gate: el
//!   `#[cfg]` de acá abajo más un `#![cfg(...)]` interno en cada archivo (el
//!   molde de `windows_integration.rs`).
//! - El crate `monarch` (los tipos del modelo) también es windows-only, así que
//!   NADA de este archivo puede mencionarlo fuera de un bloque gateado.
//!
//! # Origen
//!
//! El motor viene de Monarch @ `7f9f63b` — ver `docs/DECISIONS.md` (ADR-002) y
//! `vendor/monarch/PROVENANCE.md`.

use serde::Serialize;

#[cfg(target_os = "windows")]
mod enumerate;
#[cfg(target_os = "windows")]
mod win32_types;

/// Puente del logger del motor migrado hacia el de Millennium.
///
/// El código de Monarch llama `diagnostics::log(...)`, que allá escribía a un
/// `diagnostics.log` propio. Acá va al runtime log de Millennium, que es lo que
/// se ve en el modal LOG. Se resuelve con un shim en vez de editar los `use` del
/// motor para que el diff contra el donante siga siendo legible.
///
/// (`runtime_log` expone `err`, no `error`, y sus funciones toman
/// `impl Into<String>` — no son macros con formato inline.)
#[cfg(target_os = "windows")]
pub(crate) mod diagnostics {
    pub fn log(message: impl AsRef<str>) {
        crate::runtime_log::info(format!("[displays] {}", message.as_ref()));
    }
}

/// Un monitor, tal como lo ve el frontend.
///
/// Todo `u64` viaja como **string**: `adapter_luid` y `edid_hash` superan
/// `Number.MAX_SAFE_INTEGER` (2^53) y JavaScript los redondearía en silencio,
/// rompiendo la identidad del monitor justo en el campo que la define.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayView {
    /// Clave estable para el render por diff del frontend (`li.dataset.id`).
    pub id: String,
    pub name: String,
    pub active: bool,
    pub primary: bool,
    /// `0` en ambos ⇒ Windows no reporta modo para este monitor (está conectado
    /// pero apagado). La UI lo muestra como "—", no como "0x0".
    pub width: u32,
    pub height: u32,
    /// Refresco en **miliherz**, tal como lo entrega la CCD API (60000 = 60 Hz).
    /// El formateo a Hz lo hace el frontend.
    pub refresh_mhz: u32,
    pub position_x: i32,
    pub position_y: i32,
    pub adapter_luid: String,
    pub target_id: u32,
    pub edid_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaysSnapshot {
    /// `"windows"` = monitores reales por CCD. `"mock"` = datos de mentira
    /// (`MONARCH_FORCE_MOCK_BACKEND`). La UI lo muestra para que nadie confunda
    /// una demo con la máquina real.
    pub source: &'static str,
    pub displays: Vec<DisplayView>,
}

/// Variable de entorno que fuerza el backend falso, heredada de Monarch.
///
/// Sirve para ver la UI de displays en una máquina sin monitores raros, o en un
/// build no-Windows.
const FORCE_MOCK_ENV: &str = "MONARCH_FORCE_MOCK_BACKEND";

/// Toma la foto de los monitores.
///
/// **Bloqueante**: las llamadas CCD pueden tardar decenas de milisegundos (más si
/// un panel está despertando). El que llama TIENE que envolverla en
/// `spawn_blocking` — nunca correrla en el reactor.
pub fn snapshot() -> Result<DisplaysSnapshot, String> {
    if std::env::var_os(FORCE_MOCK_ENV).is_some() {
        crate::runtime_log::warn(format!(
            "[displays] {FORCE_MOCK_ENV} activo — devolviendo monitores FALSOS"
        ));
        return Ok(DisplaysSnapshot {
            source: "mock",
            displays: mock_displays(),
        });
    }

    #[cfg(target_os = "windows")]
    {
        let snapshot = enumerate::query_active_topology().map_err(|e| e.to_string())?;
        Ok(DisplaysSnapshot {
            source: "windows",
            displays: views_from_topology(&snapshot),
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(format!(
            "el módulo de displays solo existe en Windows (probá {FORCE_MOCK_ENV}=1 para ver datos de ejemplo)"
        ))
    }
}

/// Traduce el modelo de Monarch al DTO del frontend.
///
/// La geometría sale del `layout` (que es quien tiene la posición) y el resto de
/// `displays`. Se cruzan **por índice**, no buscando por `DisplayId`: el motor
/// construye los dos vectores en lockstep (cada `displays.push` tiene su
/// `outputs.push` en la misma vuelta, tanto en `snapshot_from_raw` como en el
/// seeder), así que el índice es la correspondencia exacta. Buscar por id sería
/// O(n²) y, ante dos monitores con el mismo `DisplayId`, le daría al segundo la
/// posición del primero. Si algún día los largos no coinciden, `get` devuelve
/// `None` y se cae al mismo default que antes.
#[cfg(target_os = "windows")]
fn views_from_topology(snapshot: &win32_types::TopologySnapshot) -> Vec<DisplayView> {
    let mut views: Vec<DisplayView> = snapshot
        .displays
        .iter()
        .enumerate()
        .map(|(idx, display)| {
            let output = snapshot.layout.outputs.get(idx);
            DisplayView {
                id: format!(
                    "{}:{}:{}",
                    display.id.adapter_luid,
                    display.id.target_id,
                    display
                        .id
                        .edid_hash
                        .map(|hash| hash.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ),
                name: if display.friendly_name.trim().is_empty() {
                    // Windows devuelve nombre vacío para algunos paneles
                    // internos. Mismo formato que usa el motor en su propio
                    // fallback (luid + target), para que dos monitores sin
                    // nombre en placas distintas no queden con la misma etiqueta.
                    format!("Display {}:{}", display.id.adapter_luid, display.id.target_id)
                } else {
                    display.friendly_name.clone()
                },
                active: display.is_active,
                primary: display.is_primary,
                width: display.resolution.width,
                height: display.resolution.height,
                refresh_mhz: display.refresh_rate_mhz,
                position_x: output.map(|o| o.position.x).unwrap_or(0),
                position_y: output.map(|o| o.position.y).unwrap_or(0),
                adapter_luid: display.id.adapter_luid.to_string(),
                target_id: display.id.target_id,
                edid_hash: display.id.edid_hash.map(|hash| hash.to_string()),
            }
        })
        .collect();

    // Antes de ordenar: así "el primero" es el primero de la enumeración de
    // Windows, no el que quedó arriba después del sort.
    keep_single_primary(&mut views);
    sort_for_display(&mut views);
    views
}

/// Deja UN solo primario.
///
/// El motor deduce `is_primary` de que la posición sea (0,0), porque Windows no
/// lo reporta directo. En **modo espejo** dos monitores clonados comparten esa
/// posición, así que los marca primarios a los dos y la UI mostraría dos badges
/// PRIMARY. Windows tiene uno solo: se conserva el primero en el orden en que
/// vino de la enumeración y se apagan los demás.
///
/// Se corrige acá, en la vista, y no en el motor migrado: la Fase 1 es de solo
/// lectura y conviene no tocarle la semántica al código que viene de Monarch.
fn keep_single_primary(views: &mut [DisplayView]) {
    let mut ya_hay = false;
    for view in views.iter_mut() {
        if view.primary {
            if ya_hay {
                view.primary = false;
            } else {
                ya_hay = true;
            }
        }
    }
}

/// Orden estable y con sentido para la UI: primario, después el resto de los
/// activos, después los desconectados. Dentro de cada grupo, por `target_id`.
///
/// Importa que sea determinista: el frontend renderiza por diff, y un orden que
/// baila haría saltar las filas en cada refresco.
fn sort_for_display(views: &mut [DisplayView]) {
    views.sort_by_key(|view| {
        let rank = if view.primary {
            0
        } else if view.active {
            1
        } else {
            2
        };
        (rank, view.target_id)
    });
}

/// Los tres monitores de mentira, copiados de `build_mock_backend()` de Monarch:
/// uno primario 1080p, uno lateral 1440p/144 Hz y uno vertical DESCONECTADO
/// (para poder ver el badge de detached sin desenchufar nada).
fn mock_displays() -> Vec<DisplayView> {
    let mut views = vec![
        DisplayView {
            id: "1:1:1".to_string(),
            name: "Primary Panel (Mock)".to_string(),
            active: true,
            primary: true,
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            position_x: 0,
            position_y: 0,
            adapter_luid: "1".to_string(),
            target_id: 1,
            edid_hash: Some("1".to_string()),
        },
        DisplayView {
            id: "1:2:2".to_string(),
            name: "Side Display (Mock)".to_string(),
            active: true,
            primary: false,
            width: 2560,
            height: 1440,
            refresh_mhz: 144_000,
            position_x: 1920,
            position_y: 0,
            adapter_luid: "1".to_string(),
            target_id: 2,
            edid_hash: Some("2".to_string()),
        },
        DisplayView {
            id: "1:3:3".to_string(),
            name: "Portrait Display (Mock)".to_string(),
            active: false,
            primary: false,
            // Mismo centinela que el motor real para un monitor sin modo activo.
            width: 0,
            height: 0,
            refresh_mhz: 60_000,
            position_x: -1080,
            position_y: 0,
            adapter_luid: "1".to_string(),
            target_id: 3,
            edid_hash: Some("3".to_string()),
        },
    ];
    sort_for_display(&mut views);
    views
}

// Los tests del crate no pueden correr en Windows: agregar un `#[test]` acá
// arrastra el stack de tao/wry al binario de tests y muere al cargar con
// STATUS_ENTRYPOINT_NOT_FOUND. Mismo gate que `json_store.rs`.
#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn mock_displays_ordena_primario_activos_y_despues_desconectados() {
        let views = mock_displays();
        assert_eq!(views.len(), 3);
        assert!(views[0].primary, "el primario va primero");
        assert!(views[1].active && !views[1].primary);
        assert!(!views[2].active, "el desconectado va último");
    }

    #[test]
    fn el_desconectado_usa_el_centinela_cero_por_cero() {
        let views = mock_displays();
        let detached = views.iter().find(|v| !v.active).expect("hay uno inactivo");
        assert_eq!((detached.width, detached.height), (0, 0));
    }

    #[test]
    fn los_u64_viajan_como_string_para_no_perder_precision_en_js() {
        // 2^53 + 1: el primer entero que un Number de JS NO puede representar.
        let big = (1u64 << 53) + 1;
        let view = DisplayView {
            id: "x".to_string(),
            name: "x".to_string(),
            active: true,
            primary: true,
            width: 1,
            height: 1,
            refresh_mhz: 60_000,
            position_x: 0,
            position_y: 0,
            adapter_luid: big.to_string(),
            target_id: 1,
            edid_hash: Some(big.to_string()),
        };
        let json = serde_json::to_string(&view).expect("serializa");
        assert!(
            json.contains(&format!("\"adapterLuid\":\"{big}\"")),
            "adapterLuid tiene que ir entre comillas, no como número: {json}"
        );
        assert!(json.contains(&format!("\"edidHash\":\"{big}\"")));
    }

    #[test]
    fn el_snapshot_mock_se_activa_por_la_env_var() {
        std::env::set_var(FORCE_MOCK_ENV, "1");
        let snapshot = snapshot().expect("el mock nunca falla");
        std::env::remove_var(FORCE_MOCK_ENV);
        assert_eq!(snapshot.source, "mock");
        assert_eq!(snapshot.displays.len(), 3);
    }
}
