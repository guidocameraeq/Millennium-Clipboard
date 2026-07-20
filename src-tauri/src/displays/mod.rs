//! Módulo de displays — **Fase 2 del SPEC-displays: ver Y mover, con red.**
//!
//! Enumera los monitores conectados (activos y desconectados-pero-presentes),
//! y prende o apaga uno con una **red de auto-rollback**: si el cambio no se
//! confirma dentro del plazo, se revierte solo.
//!
//! # Las dos piezas de la red (leer antes de tocar nada)
//!
//! 1. `watchdog.rs` — el gatillo que revierte al vencer el plazo. El manager del
//!    crate puro guarda el plazo pero es **pasivo**: sin este gatillo, un layout
//!    malo queda pegado para siempre. Es el bug que Monarch nació para matar.
//! 2. `system_events.rs` — al despertar la máquina, tira el cache (los LUID de
//!    las placas cambian al suspender).
//!
//! Y la regla que las gobierna a las dos, heredada de Monarch: **nunca se juzga
//! por el código que devuelve Windows, siempre se verifica re-enumerando.** Un
//! `SetDisplayConfig` que no cambió nada devuelve "éxito" igual.
//!
//! # Forma del módulo
//!
//! - Este `mod.rs` es **ungateado** a propósito: los tipos que cruzan al frontend
//!   y el comando tienen que poder nombrarse en cualquier plataforma. Si el DTO
//!   viviera tras `cfg(windows)`, la firma del comando no compilaría en Android.
//! - Los submódulos son **windows-only**, con doble gate: el `#[cfg]` de acá
//!   abajo más un `#![cfg(...)]` interno en cada archivo (el molde de
//!   `windows_integration.rs`).
//! - El crate `monarch` (los tipos del modelo) también es windows-only, así que
//!   NADA de este archivo puede mencionarlo fuera de un bloque gateado.
//! - **Este módulo no menciona a Tauri.** La glue que emite eventos vive en
//!   `lib.rs` y entra por el callback `Emisor`. No es purismo: es lo que permite
//!   type-checkear todo el motor localmente en un crate scratch, porque el stack
//!   de Tauri no compila en la máquina del dueño (ver `docs/DECISIONS.md`).
//!
//! # Origen
//!
//! El motor viene de Monarch @ `7f9f63b` — ver `docs/DECISIONS.md` (ADR-002) y
//! `vendor/monarch/PROVENANCE.md`.

use serde::Serialize;

#[cfg(target_os = "windows")]
#[allow(dead_code)] // helpers de color/wallpaper que solo usa una rama del apply
mod apply;
#[cfg(target_os = "windows")]
mod backend;
#[cfg(target_os = "windows")]
mod enumerate;
#[cfg(target_os = "windows")]
mod ids;
#[cfg(target_os = "windows")]
mod store;
#[cfg(target_os = "windows")]
mod system_events;
#[cfg(target_os = "windows")]
#[allow(dead_code)] // ídem: parte del motor donante no la ejerce el toggle
mod topology;
#[cfg(target_os = "windows")]
mod watchdog;
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
    /// `false` cuando apagar este monitor dejaría la máquina sin ninguna
    /// pantalla. La UI deshabilita el botón DETACH; el manager además lo rechaza
    /// por su cuenta, así que la guarda existe por duplicado a propósito: acá
    /// para no ofrecer un botón que no puede funcionar, allá para que ninguna
    /// otra vía de entrada se saltee la regla.
    ///
    /// Solo habla de **apagar**. En una fila ya apagada no se mira: si se
    /// interpretara ahí, ATTACH quedaría muerto y el monitor no se podría
    /// volver a prender nunca.
    pub can_detach: bool,
}

/// Lo que falta para que un cambio sin confirmar se revierta solo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    /// Milisegundos que quedan, según el reloj del **backend** — que es el que
    /// manda. La UI lo usa para rehidratar la cuenta regresiva si el usuario
    /// cerró y reabrió el modal en el medio.
    pub remaining_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaysSnapshot {
    /// `"windows"` = monitores reales por CCD. `"mock"` = datos de mentira
    /// (`MONARCH_FORCE_MOCK_BACKEND`). La UI lo muestra para que nadie confunda
    /// una demo con la máquina real.
    pub source: &'static str,
    pub displays: Vec<DisplayView>,
    /// `None` = no hay nada esperando confirmación.
    pub pending: Option<PendingView>,
}

/// Marca qué monitores se pueden apagar: solo los activos, y solo si no son el
/// último que queda prendido.
///
/// Se calcula sobre la lista ya armada en vez de preguntarle al manager, así
/// vale igual para el camino de lectura de la Fase 1 (que no tiene manager) y
/// para el del apply.
fn mark_can_detach(views: &mut [DisplayView]) {
    let activos = views.iter().filter(|v| v.active).count();
    for view in views.iter_mut() {
        view.can_detach = view.active && activos > 1;
    }
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
            pending: None,
        });
    }

    #[cfg(target_os = "windows")]
    {
        let snapshot = enumerate::query_active_topology().map_err(|e| e.to_string())?;
        Ok(DisplaysSnapshot {
            source: "windows",
            displays: views_from_topology(&snapshot),
            // Esta función no conoce el manager (es el camino de lectura puro de
            // la Fase 1, intacto). Quien tenga estado rellena el pendiente.
            pending: None,
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
                // Lo decide `mark_can_detach` abajo, que necesita ver la lista
                // entera para saber cuántos quedan prendidos.
                can_detach: false,
            }
        })
        .collect();

    // Antes de ordenar: así "el primero" es el primero de la enumeración de
    // Windows, no el que quedó arriba después del sort.
    keep_single_primary(&mut views);
    mark_can_detach(&mut views);
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
// Solo la llama `views_from_topology`, que es windows-only: sin el gate, la rama
// no-Windows la reporta como código muerto y esa advertencia tapa las de verdad.
#[cfg(target_os = "windows")]
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
            can_detach: false, // lo calcula mark_can_detach
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
            can_detach: false,
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
            can_detach: false,
        },
    ];
    mark_can_detach(&mut views);
    sort_for_display(&mut views);
    views
}

// ---------------------------------------------------------------------------
// FASE 2 — el estado, el apply y la red de seguridad
// ---------------------------------------------------------------------------

/// Nombres de los eventos que el frontend escucha.
pub const EVENTO_CAMBIO: &str = "displays-changed";
pub const EVENTO_CONFIRMACION: &str = "displays-confirmation";

/// Cómo este módulo le habla al frontend, **sin conocer a Tauri**.
///
/// `lib.rs` inyecta un closure que hace `app.emit(nombre, payload)`. Mantener el
/// tipo acá y la implementación allá es lo que permite type-checkear todo el
/// motor en el crate scratch local (el stack de Tauri no compila en esta
/// máquina). No es prolijidad: es la diferencia entre verificar antes del CI o
/// después.
#[cfg(target_os = "windows")]
pub type Emisor = Box<dyn Fn(&str, serde_json::Value) + Send + Sync + 'static>;

#[cfg(target_os = "windows")]
mod estado {
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use monarch::{Layout, MonarchDisplayManager};

    use super::backend::SystemDisplayBackend;
    use super::ids::{format_display_id, parse_display_id};
    use super::store::MillenniumConfigStore;
    use super::watchdog::{self, Desenlace};
    use super::{
        diagnostics, mark_can_detach, DisplayView, DisplaysSnapshot, Emisor, PendingView,
        EVENTO_CAMBIO, EVENTO_CONFIRMACION, FORCE_MOCK_ENV,
    };

    type Manager = MonarchDisplayManager<SystemDisplayBackend, MillenniumConfigStore>;

    /// Plazo de gracia si el manager no supiera decir cuánto falta. No debería
    /// pasar (siempre hay pendiente después de un apply exitoso), pero un
    /// watchdog sin plazo sería un watchdog que no arranca.
    const PLAZO_DE_EMERGENCIA: Duration = Duration::from_secs(10);

    pub struct Interno {
        manager: Mutex<Manager>,
        emisor: Emisor,
        /// `true` cuando se está corriendo contra monitores de mentira.
        mock: bool,
    }

    /// El handle que se guarda en Tauri y se clona hacia los hilos.
    ///
    /// Es `Arc` y no un `State<'_>` prestado a propósito: el trabajo pesado
    /// corre en `spawn_blocking` y el watchdog en un hilo propio, y los dos
    /// necesitan quedarse con el estado más allá del comando que los lanzó.
    #[derive(Clone)]
    pub struct DisplaysState(Arc<Interno>);

    /// Arranca el motor. **Nunca es fatal**: si esto devuelve `Err`, `lib.rs` lo
    /// loguea y Millennium sigue andando sin la sección de monitores.
    pub fn init(data_dir: &Path, emisor: Emisor) -> Result<DisplaysState, String> {
        let mock = std::env::var_os(FORCE_MOCK_ENV).is_some();
        let backend = SystemDisplayBackend::new().map_err(|e| e.to_string())?;
        let store = MillenniumConfigStore::new(data_dir).map_err(|e| e.to_string())?;
        if store.loaded_from_corrupt() {
            crate::runtime_log::err(
                "[displays] la config de displays no parseaba; se respaldó y se arrancó con los valores por defecto",
            );
        }
        diagnostics::log(format!("init:store={}", store.config_path().display()));

        let manager = MonarchDisplayManager::new(backend, store).map_err(|e| e.to_string())?;
        let estado = DisplaysState(Arc::new(Interno {
            manager: Mutex::new(manager),
            emisor,
            mock,
        }));

        // Segunda pieza de la red: al despertar la máquina, tirar el cache.
        let para_el_resume = estado.clone();
        super::system_events::spawn(Box::new(move || para_el_resume.al_despertar()));

        Ok(estado)
    }

    impl DisplaysState {
        // --- lectura ---------------------------------------------------------

        /// La foto que va al frontend, con el estado de la confirmación pendiente.
        pub fn snapshot(&self) -> Result<DisplaysSnapshot, String> {
            let mut foto = self.foto_cruda()?;
            foto.pending = self.pendiente();
            Ok(foto)
        }

        /// La foto SIN el pendiente.
        ///
        /// En Windows sale de una **enumeración fresca de la CCD API**, no del
        /// cache del motor: es a propósito. Esta misma función es la que verifica
        /// que un apply haya hecho lo que decía, y verificar contra el cache
        /// propio sería preguntarle al acusado.
        fn foto_cruda(&self) -> Result<DisplaysSnapshot, String> {
            if self.0.mock {
                return self.foto_del_manager();
            }
            super::snapshot()
        }

        /// En modo mentira la foto sale del manager, para que el ensayo del
        /// attach/detach se vea reflejado en la lista. (El camino de lectura de
        /// la Fase 1 devuelve monitores fijos, que nunca cambiarían.)
        fn foto_del_manager(&self) -> Result<DisplaysSnapshot, String> {
            let guard = self.0.manager.lock().map_err(|_| envenenado())?;
            let displays = guard.list_displays().map_err(|e| e.to_string())?;
            let layout = guard.get_layout().map_err(|e| e.to_string())?;
            drop(guard);
            Ok(DisplaysSnapshot {
                source: "mock",
                displays: vistas_del_modelo(&displays, &layout),
                pending: None,
            })
        }

        fn pendiente(&self) -> Option<PendingView> {
            let guard = self.0.manager.lock().ok()?;
            let restante = guard.pending_confirmation_remaining()?;
            Some(PendingView {
                remaining_ms: restante.as_millis() as u64,
            })
        }

        // --- apply -----------------------------------------------------------

        /// Prende o apaga un monitor, con la red puesta.
        ///
        /// El orden importa y es el de la doctrina CCD:
        /// 1. Se mira el estado actual para saber qué se está pidiendo.
        /// 2. Se aplica (el manager guarda el layout anterior y arranca el plazo).
        /// 3. **Se verifica RE-ENUMERANDO**, nunca por el código que devolvió
        ///    Windows: un `SetDisplayConfig` que no cambió nada devuelve éxito.
        /// 4. Si no pasó lo que se pidió, se revierte **en el acto** — dejar una
        ///    configuración a medias armada esperando diez segundos es peor.
        /// 5. Recién con el cambio confirmado por enumeración se arma el watchdog.
        pub fn toggle(&self, display_id: &str) -> Result<DisplaysSnapshot, String> {
            let id = parse_display_id(display_id)?;

            let antes = self.foto_cruda()?;
            let estaba_activo = antes
                .displays
                .iter()
                .find(|v| v.id == display_id)
                .map(|v| v.active)
                .ok_or_else(|| {
                    format!("ese monitor ya no está en la lista (id {display_id}) — refrescá")
                })?;
            let se_quiere_activo = !estaba_activo;

            let plazo = {
                let mut guard = self.0.manager.lock().map_err(|_| envenenado())?;
                guard.toggle_display(&id).map_err(|e| e.to_string())?;
                guard
                    .pending_confirmation_remaining()
                    .unwrap_or(PLAZO_DE_EMERGENCIA)
            };

            // ---------------------------------------------------------------
            // A PARTIR DE ACÁ EL CAMBIO YA ESTÁ APLICADO.
            //
            // Regla de este bloque: **ninguna salida puede irse sin dejar armado
            // el watchdog**, salvo que la confirmación pendiente ya no exista.
            // Por eso no hay un solo `?` de acá abajo: un `?` sería un `return`
            // silencioso que deja el cambio puesto y a nadie persiguiéndolo, que
            // es literalmente el bug que esta fase viene a matar.
            // ---------------------------------------------------------------

            // --- la verificación que la doctrina exige: RE-ENUMERAR ---
            let despues = match self.foto_cruda() {
                Ok(foto) => foto,
                Err(err) => {
                    // No se pudo mirar cómo quedó. Pasa de verdad: justo después
                    // de mover la topología, la CCD API puede rebotar mientras el
                    // stack de video se asienta.
                    //
                    // Sin verificación no se puede afirmar que el cambio salió
                    // bien, así que se hace lo conservador: se deja la red puesta.
                    // Si el usuario ve su pantalla como quería, confirma y listo;
                    // si no, en unos segundos vuelve sola.
                    crate::runtime_log::warn(format!(
                        "[displays] no se pudo verificar cómo quedó ({err}); el watchdog queda armado, así que si no confirmás vuelve solo"
                    ));
                    self.armar_watchdog(plazo);
                    self.avisar_confirmacion(serde_json::json!({
                        "kind": "applied",
                        "timeoutMs": plazo.as_millis() as u64,
                    }));
                    self.avisar_cambio();
                    return Err(format!(
                        "el cambio se aplicó pero no se pudo verificar cómo quedó ({err}). Si la pantalla está bien, confirmá; si no, esperá y vuelve sola."
                    ));
                }
            };
            let quedo_activo = despues
                .displays
                .iter()
                .find(|v| v.id == display_id)
                .map(|v| v.active);

            if quedo_activo != Some(se_quiere_activo) {
                let que_paso = match quedo_activo {
                    Some(v) => format!("quedó active={v}"),
                    None => "desapareció de la enumeración".to_string(),
                };
                crate::runtime_log::err(format!(
                    "[displays] el cambio NO tomó efecto: se pidió active={se_quiere_activo} y {que_paso}. Revirtiendo ya."
                ));
                // Sin `?`: ver la regla del bloque de arriba. Si ni siquiera se
                // puede tomar el lock, se trata como "el rollback falló" y sigue
                // por la rama que deja el watchdog armado.
                let detalle_rollback = match self.0.manager.lock() {
                    Ok(mut guard) => guard.rollback_pending().err().map(|e| e.to_string()),
                    Err(_) => Some(envenenado()),
                };
                self.avisar_cambio();
                return Err(match detalle_rollback {
                    None => {
                        self.avisar_confirmacion(serde_json::json!({
                            "kind": "reverted",
                            "reason": "error",
                            "detail": "el cambio no tomó efecto",
                        }));
                        format!(
                            "Windows aceptó el cambio pero el monitor {que_paso}. Se revirtió solo."
                        )
                    }
                    Some(err) => {
                        // La vuelta atrás inmediata falló. El manager CONSERVA la
                        // confirmación pendiente cuando eso pasa (no la tira), así
                        // que todavía hay algo que revertir — y si nos fuéramos
                        // acá, nadie lo perseguiría nunca y el layout malo quedaría
                        // pegado. Se arma el watchdog igual: es exactamente el
                        // escenario para el que existe.
                        crate::runtime_log::err(format!(
                            "[displays] la vuelta atrás inmediata falló ({err}); queda el watchdog persiguiéndola"
                        ));
                        self.armar_watchdog(plazo);
                        self.avisar_confirmacion(serde_json::json!({
                            "kind": "applied",
                            "timeoutMs": plazo.as_millis() as u64,
                        }));
                        format!(
                            "Windows aceptó el cambio pero el monitor {que_paso}, y la vuelta atrás inmediata también falló: {err}. Se sigue intentando revertir solo."
                        )
                    }
                });
            }

            self.armar_watchdog(plazo);
            self.avisar_confirmacion(serde_json::json!({
                "kind": "applied",
                "timeoutMs": plazo.as_millis() as u64,
            }));
            self.avisar_cambio();

            let mut foto = despues;
            foto.pending = self.pendiente();
            Ok(foto)
        }

        /// El usuario dice "así está bien": se cancela el auto-rollback.
        pub fn confirm(&self) -> Result<DisplaysSnapshot, String> {
            {
                let mut guard = self.0.manager.lock().map_err(|_| envenenado())?;
                guard.confirm_current_layout().map_err(|e| e.to_string())?;
            }
            diagnostics::log("confirm:el_usuario_confirmo");
            self.avisar_confirmacion(serde_json::json!({ "kind": "confirmed" }));
            self.avisar_cambio();
            self.snapshot()
        }

        /// El usuario dice "volvé atrás" sin esperar a que venza el plazo.
        pub fn revert(&self) -> Result<DisplaysSnapshot, String> {
            {
                let mut guard = self.0.manager.lock().map_err(|_| envenenado())?;
                guard.rollback_pending().map_err(|e| e.to_string())?;
            }
            diagnostics::log("revert:a_pedido_del_usuario");
            self.avisar_confirmacion(serde_json::json!({
                "kind": "reverted",
                "reason": "manual",
            }));
            self.avisar_cambio();
            self.snapshot()
        }

        // --- la red ----------------------------------------------------------

        /// Lanza el gatillo del auto-rollback.
        ///
        /// Hilo propio con `thread::sleep`, **no** una tarea de Tokio: el
        /// rollback toma un `std::sync::Mutex` y hace llamadas CCD bloqueantes,
        /// así que la regla "nunca sostener un lock a través de un `.await`" se
        /// cumple por construcción — acá no hay ningún `.await`.
        fn armar_watchdog(&self, plazo: Duration) {
            let estado = self.clone();
            std::thread::spawn(move || {
                watchdog::correr(
                    &estado.0.manager,
                    plazo,
                    &mut std::thread::sleep,
                    &mut |desenlace| estado.reportar_desenlace(desenlace),
                );
            });
        }

        fn reportar_desenlace(&self, desenlace: Desenlace) {
            match desenlace {
                Desenlace::Revertido => {
                    crate::runtime_log::warn(
                        "[displays] nadie confirmó el cambio: se revirtió solo",
                    );
                    self.avisar_confirmacion(serde_json::json!({
                        "kind": "reverted",
                        "reason": "timeout",
                    }));
                    self.avisar_cambio();
                }
                // Alguien resolvió la confirmación antes (confirmó o revirtió a
                // mano). Los eventos ya los mandó ese camino; repetirlos acá haría
                // parpadear la UI.
                Desenlace::NadaQueHacer => {}
                Desenlace::NoPudoRevertir(motivo) => {
                    crate::runtime_log::err(format!(
                        "[displays] NO se pudo revertir solo: {motivo}. La pantalla quedó como está; revertí a mano."
                    ));
                    self.avisar_confirmacion(serde_json::json!({
                        "kind": "reverted",
                        "reason": "error",
                        "detail": motivo,
                    }));
                    self.avisar_cambio();
                }
            }
        }

        /// Al despertar la máquina: tirar el cache del motor.
        fn al_despertar(&self) {
            let resultado = match self.0.manager.lock() {
                Ok(guard) => guard.invalidate_backend_cache(),
                Err(_) => {
                    crate::runtime_log::err("[displays] resume: el estado quedó inconsistente");
                    return;
                }
            };
            if let Err(err) = resultado {
                crate::runtime_log::warn(format!("[displays] resume: no se pudo tirar el cache: {err}"));
            }
            self.avisar_cambio();
        }

        // --- avisos ----------------------------------------------------------

        fn avisar_cambio(&self) {
            (self.0.emisor)(EVENTO_CAMBIO, serde_json::Value::Null);
        }

        fn avisar_confirmacion(&self, payload: serde_json::Value) {
            (self.0.emisor)(EVENTO_CONFIRMACION, payload);
        }
    }

    fn envenenado() -> String {
        "el estado de monitores quedó inconsistente; reiniciá Millennium".to_string()
    }

    /// Arma las vistas desde el modelo de Monarch (no desde los tipos Win32).
    ///
    /// Se usa en modo mentira, donde no hay CCD que enumerar. Cruza por
    /// `DisplayId` y no por índice porque `list_displays()` y `get_layout()` son
    /// dos consultas separadas al backend, sin la correspondencia en lockstep que
    /// sí tiene el snapshot crudo.
    fn vistas_del_modelo(displays: &[monarch::DisplayInfo], layout: &Layout) -> Vec<DisplayView> {
        let mut views: Vec<DisplayView> = displays
            .iter()
            .map(|display| {
                let output = layout
                    .outputs
                    .iter()
                    .find(|output| output.display_id == display.id);
                DisplayView {
                    id: format_display_id(&display.id),
                    name: display.friendly_name.clone(),
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
                    can_detach: false,
                }
            })
            .collect();
        mark_can_detach(&mut views);
        super::sort_for_display(&mut views);
        views
    }

}

#[cfg(target_os = "windows")]
pub use estado::{init, DisplaysState};

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
            can_detach: true,
        };
        let json = serde_json::to_string(&view).expect("serializa");
        assert!(
            json.contains(&format!("\"adapterLuid\":\"{big}\"")),
            "adapterLuid tiene que ir entre comillas, no como número: {json}"
        );
        assert!(json.contains(&format!("\"edidHash\":\"{big}\"")));
    }

    #[test]
    fn el_ultimo_monitor_prendido_no_se_puede_apagar() {
        // El mock tiene dos activos, así que los dos se pueden apagar.
        let views = mock_displays();
        for view in views.iter().filter(|v| v.active) {
            assert!(view.can_detach, "con dos prendidos, {} se puede apagar", view.name);
        }
        // Y ninguno apagado ofrece DETACH.
        for view in views.iter().filter(|v| !v.active) {
            assert!(!view.can_detach);
        }

        // Con uno solo prendido, ese uno NO se puede apagar: es la guarda que
        // impide dejar la máquina a ciegas.
        let mut solo_uno: Vec<DisplayView> = views
            .into_iter()
            .map(|mut v| {
                v.active = v.primary;
                v
            })
            .collect();
        mark_can_detach(&mut solo_uno);
        assert!(
            solo_uno.iter().all(|v| !v.can_detach),
            "con un solo monitor prendido, nadie puede apagarse"
        );
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
