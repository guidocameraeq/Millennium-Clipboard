// El backend de Windows: el cache de topología y la escalera de rescate.
// Migrado de Monarch @ 7f9f63b (`src-tauri/src/backend/windows/topology.rs`) —
// ver docs/DECISIONS.md ADR-002 y la sección "Doctrina CCD heredada".
//
// FASE 2 del SPEC-displays. Es la pieza que la Fase 1 dejó afuera entera.
//
// # Las dos cosas que hace este archivo, y por qué no se pueden simplificar
//
// 1. **El cache** (`Mutex<BackendCache>` + `refresh_active`). Windows deja de
//    reportar un monitor apenas se apaga o se detacha. Si la lista saliera
//    derecho de la enumeración, el monitor desaparecería de la UI justo cuando
//    el usuario quiere volver a prenderlo. El cache lo mantiene visible —con su
//    última geometría conocida— y es lo que hace posible el re-attach. Toda la
//    familia de `merge_*` de acá abajo existe para eso: fusionar lo que Windows
//    dice AHORA con lo que sabíamos ANTES, sin que un dato viejo le gane a uno
//    fresco ni al revés.
//
// 2. **La escalera de rescate** (`recover_apply_with_topology_extend`). Cuatro
//    escalones, en orden, cada uno confirmado re-enumerando: attach explícito →
//    topology extend → DisplaySwitch → rollback + error honesto. Ningún escalón
//    cree en el código de retorno de `SetDisplayConfig` (un no-op también
//    devuelve 0). Es la doctrina CCD heredada, y costó meses.
//
// # Qué se dejó afuera del donante: la persistencia binaria del snapshot
//
// El donante guardaba el array crudo de `DISPLAYCONFIG_PATH_INFO`/`MODE_INFO` en
// un archivo JSON (`topology_snapshot.json`) y lo releía al arrancar y después
// de cada invalidación. Acá **no viaja nada de eso**: se fueron
// `PersistedRawSnapshot`, `persist_raw_snapshot`, `load_persisted_raw_snapshot`,
// `struct_to_bytes`/`struct_from_bytes` y `merge_persisted_raw_for_fresh`.
//
// Dos motivos, los dos duros:
//
//   (a) `struct_from_bytes` hacía `MaybeUninit::assume_init()` sobre bytes
//       leídos de un archivo validando **solo el largo**. Esos structs iban
//       derecho a `SetDisplayConfig`. Millennium compila release con
//       `panic = "abort"`: un archivo corrupto o de otra versión del driver no
//       daba un `Err`, daba estructuras basura mandadas al kernel gráfico.
//   (b) La ruta salía de `monarch::FileConfigStore::default_config_path()`, o
//       sea `%APPDATA%\Monarch\` — **la configuración real de Monarch del
//       usuario**. Escribir ahí desde Millennium es pisarle datos suyos.
//
// Qué se pierde: el recuerdo de un monitor detachado **no sobrevive a un
// reinicio de la app**. Dentro de la sesión el cache en memoria lo mantiene
// igual que antes, y los candidatos de attach nunca salieron del archivo — salen
// de la enumeración viva `QDC_ALL_PATHS` (`snapshot.attachable`), que es lo que
// de verdad permite re-adjuntar. Al arrancar de nuevo, la primera enumeración
// vuelve a sembrar los conectados-pero-inactivos.
#![cfg(target_os = "windows")]

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use super::diagnostics;
use monarch::{DisplayBackend, DisplayId, DisplayInfo, Layout, ManagerError};

use super::apply::{
    active_color_state_signature, apply_attach_paths, apply_layout_against_snapshot,
    build_attach_paths, capture_sdr_gamma_ramps, gamma_ramp_looks_identity,
    reapply_color_calibration_for_active_with_cached_sdr, run_display_switch_extend,
    try_topology_extend, validate_attach_paths, GammaRampKey, GammaRampWords,
};
use super::enumerate::{query_active_only_topology, query_active_topology};
use super::win32_types::{luid_to_u64, AttachablePath, RawTopologySnapshot, TopologySnapshot};

const DISPLAYCONFIG_PATH_ACTIVE_FLAG: u32 = 0x0000_0001;

/// Mensaje único para un `Mutex` envenenado. Nunca `unwrap()` sobre un lock: el
/// `unwrap` de un lock envenenado es un panic, y con `panic = "abort"` eso se
/// lleva puesto el portapapeles entero.
fn cache_poisoned() -> ManagerError {
    ManagerError::Backend("windows backend cache poisoned".to_string())
}

/// Lo que el backend recuerda entre consultas.
///
/// Sin esto, un monitor apagado desaparece de la lista y no hay forma de
/// pedirle a Windows que vuelva.
#[derive(Default)]
struct BackendCache {
    last_snapshot: Option<TopologySnapshot>,
    last_layout: Option<Layout>,
    last_displays: Vec<DisplayInfo>,
    /// Las rampas gamma SDR por target. Va aparte del resto porque **no** se
    /// tira en `invalidate_cache`: la calibración de color no la renumera
    /// Windows al despertar, y perderla deja las pantallas lavadas.
    sdr_gamma_cache: HashMap<GammaRampKey, GammaRampWords>,
}

#[derive(Default)]
pub struct WindowsDisplayBackend {
    cache: Mutex<BackendCache>,
}

impl WindowsDisplayBackend {
    /// Arranca el backend con una enumeración fresca ya cacheada.
    ///
    /// El donante, además, intentaba rellenar este primer snapshot con el
    /// archivo persistido de la corrida anterior. Acá no hay archivo (ver el
    /// encabezado), así que la primera foto es la que da Windows y punto.
    pub fn new() -> Result<Self, ManagerError> {
        let backend = Self::default();
        let snapshot = query_active_topology()?;
        let initial_sdr_ramps = capture_sdr_gamma_ramps(&snapshot);

        let mut cache = backend.cache.lock().map_err(|_| cache_poisoned())?;
        cache.last_layout = Some(snapshot.layout.clone());
        cache.last_displays = snapshot.displays.clone();
        cache.last_snapshot = Some(snapshot);
        merge_sdr_gamma_cache(&mut cache.sdr_gamma_cache, initial_sdr_ramps);
        drop(cache);

        Ok(backend)
    }

    /// Tira todo lo cacheado (snapshot, layout y displays) para que la próxima
    /// consulta lo reconstruya desde una enumeración limpia. El cache de gamma
    /// SDR se conserva a propósito.
    ///
    /// Lo llama el resume-listener cuando la máquina despierta: ahí Windows pudo
    /// haber renumerado las placas (`adapter_luid` nuevos) y la enumeración de
    /// ese instante suele fallar al leer los EDID —los paneles recién arrancan—,
    /// así que lo guardado apunta a identidades que ya no existen.
    ///
    /// **No** se llama ante `WM_DISPLAYCHANGE`: un apply propio dispara ese
    /// mensaje, y borrar el cache ahí borraría justo el monitor que se acaba de
    /// apagar, o sea la posibilidad de volver a prenderlo.
    pub fn invalidate_cache(&self) -> Result<(), ManagerError> {
        let mut cache = self.cache.lock().map_err(|_| cache_poisoned())?;
        cache.last_snapshot = None;
        cache.last_layout = None;
        cache.last_displays.clear();
        drop(cache);
        diagnostics::log("backend_cache:invalidated");
        Ok(())
    }

    /// Gancho de diagnóstico que el manager llama **antes** de rechazar un
    /// perfil cuyos outputs habilitados no resuelven. A propósito NO toca la
    /// topología: solo deja registrado POR QUÉ el monitor no se puede usar.
    ///
    /// Por qué acá no hay rescate posible: que un output "resuelva" quiere decir
    /// que el monitor está **enumerado**, no que esté activo. El seeder de
    /// `QDC_ALL_PATHS` ya mete en el layout a todos los conectados-pero-apagados
    /// (como `is_active=false`), así que cualquier cosa re-adjuntable ya
    /// resuelve; al revés, un output que NO resuelve nombra un monitor que
    /// Windows no está enumerando en absoluto — apagado, desenchufado, o
    /// reportado con `targetAvailable=FALSE`. Ninguna maniobra lo conjura: el
    /// attach explícito solo levanta la bandera ACTIVE de un path que ya tiene
    /// que existir, y `SDC_TOPOLOGY_EXTEND` reproduce la base de persistencia.
    ///
    /// Esto antes forzaba un extend. En la cancha eso solo producía un cambio de
    /// topología que nadie pidió —prendía todos los conectados-inactivos,
    /// incluida la TV que el perfil venía a APAGAR— más 3,5 s de freno, para
    /// terminar fallando igual con `ERROR_GEN_FAILURE`.
    pub fn prepare_attach_targets(&self, desired: &Layout) -> Result<(), ManagerError> {
        let snapshot = query_active_topology()?;
        let unresolved = unresolved_enabled_outputs(desired, &snapshot);
        if unresolved.is_empty() {
            return Ok(());
        }

        let used_source_keys = active_source_keys(&snapshot);
        for output in &unresolved {
            let description = describe_output_for_error(output, &snapshot);
            let candidates =
                select_attach_candidates(&snapshot.attachable, &output.display_id, &used_source_keys);
            if candidates.is_empty() {
                diagnostics::log(format!(
                    "prepare_attach_targets:no_candidate:{description}:skip_extend"
                ));
            } else {
                // Canario: que haya candidato de attach para un output que no
                // resuelve significa que ese conector ahora tiene un monitor con
                // otra identidad que la que el perfil espera (le cambió el EDID).
                // Prenderlo no haría que el perfil resuelva, así que la topología
                // igual se deja quieta.
                diagnostics::log(format!(
                    "prepare_attach_targets:candidate_for_unresolved_output:{description}:skip_extend"
                ));
            }
        }
        Ok(())
    }

    /// Enumera y fusiona contra lo cacheado. Es el único lugar que actualiza el
    /// cache fuera del apply.
    fn refresh_active(&self) -> Result<(), ManagerError> {
        let snapshot = query_active_topology()?;
        let fresh_connectors = raw_path_connectors(&snapshot.raw);
        let mut cache = self.cache.lock().map_err(|_| cache_poisoned())?;

        cache.last_snapshot = Some(merge_snapshot_for_cache(
            cache.last_snapshot.as_ref(),
            snapshot.clone(),
        ));
        cache.last_layout = Some(merge_layout_with_fresh(
            cache.last_layout.as_ref(),
            &snapshot.layout,
            &fresh_connectors,
        ));
        cache.last_displays =
            merge_displays_with_fresh(&cache.last_displays, &snapshot.displays, &fresh_connectors);
        Ok(())
    }

    pub fn reapply_color_calibration(&self) -> Result<(), ManagerError> {
        // El lock se toma, se clona y se suelta ANTES de la llamada larga: nada
        // de sostenerlo mientras se habla con Win32.
        let cached_sdr = {
            let cache = self.cache.lock().map_err(|_| cache_poisoned())?;
            cache.sdr_gamma_cache.clone()
        };

        reapply_color_calibration_for_active_with_cached_sdr(&cached_sdr)?;
        let refreshed_snapshot = query_active_topology()?;

        let mut cache = self.cache.lock().map_err(|_| cache_poisoned())?;
        merge_sdr_gamma_cache(
            &mut cache.sdr_gamma_cache,
            capture_sdr_gamma_ramps(&refreshed_snapshot),
        );
        Ok(())
    }

    pub fn color_state_signature(&self) -> Result<Option<String>, ManagerError> {
        let snapshot = query_active_topology()?;
        Ok(Some(active_color_state_signature(&snapshot)))
    }
}

/// Conserva el snapshot crudo viejo cuando todavía cubre los outputs activos de
/// ahora **y** tiene más paths que el fresco.
///
/// Ese "más paths" es el path del monitor recién detachado: es lo que permite
/// volver a prenderlo sin depender de que Windows lo vuelva a ofrecer.
fn merge_snapshot_for_cache(
    previous: Option<&TopologySnapshot>,
    fresh: TopologySnapshot,
) -> TopologySnapshot {
    let Some(previous) = previous else {
        return fresh;
    };

    if previous.raw.paths.len() > fresh.raw.paths.len()
        && raw_covers_active_outputs_raw(&previous.raw, &fresh.layout)
    {
        let mut merged = fresh;
        merged.raw = previous.raw.clone();
        return merged;
    }

    fresh
}

/// ¿El array crudo `raw` tiene un path para **todos** los outputs habilitados de
/// `layout`? Es la condición para poder reusarlo: un snapshot al que le falta un
/// activo, aplicado tal cual, apagaría ese monitor.
fn raw_covers_active_outputs_raw(raw: &RawTopologySnapshot, layout: &Layout) -> bool {
    layout
        .outputs
        .iter()
        .filter(|output| output.enabled)
        .all(|output| {
            raw.paths.iter().any(|path| {
                let adapter_luid = luid_to_u64(
                    path.targetInfo.adapterId.HighPart,
                    path.targetInfo.adapterId.LowPart,
                );
                adapter_luid == output.display_id.adapter_luid
                    && path.targetInfo.id == output.display_id.target_id
            })
        })
}

/// Los conectores `(adapter_luid, target_id)` que aparecen en una enumeración.
/// Es la respuesta a "¿este monitor todavía existe para Windows?".
fn raw_path_connectors(raw: &RawTopologySnapshot) -> HashSet<(u64, u32)> {
    raw.paths
        .iter()
        .map(|path| {
            (
                luid_to_u64(
                    path.targetInfo.adapterId.HighPart,
                    path.targetInfo.adapterId.LowPart,
                ),
                path.targetInfo.id,
            )
        })
        .collect()
}

/// Una entrada cacheada es un duplicado viejo cuando el MISMO monitor físico
/// (mismo hash de EDID) aparece en la foto fresca bajo otro
/// `(adapter_luid, target_id)` **y** el conector cacheado ya no existe en la foto
/// fresca (típico de la renumeración de placas después de suspender o reiniciar).
///
/// Dos monitores gemelos con EDID idéntico quedan protegidos: los dos conectores
/// siguen existiendo, así que ninguno se descarta.
fn cached_id_is_stale_duplicate<'a>(
    cached: &DisplayId,
    mut fresh_ids: impl Iterator<Item = &'a DisplayId>,
    fresh_connectors: &HashSet<(u64, u32)>,
) -> bool {
    let Some(edid_hash) = cached.edid_hash else {
        return false;
    };
    let cached_connector = (cached.adapter_luid, cached.target_id);
    if fresh_connectors.contains(&cached_connector) {
        return false;
    }
    fresh_ids.any(|fresh| {
        fresh.edid_hash == Some(edid_hash)
            && (fresh.adapter_luid, fresh.target_id) != cached_connector
    })
}

/// Los outputs habilitados de `desired` que, aun después del remapeo, no
/// resuelven contra la enumeración de `snapshot`. Mismo criterio que usa el
/// manager antes de rechazar un perfil.
fn unresolved_enabled_outputs(
    desired: &Layout,
    snapshot: &TopologySnapshot,
) -> Vec<monarch::OutputConfig> {
    let remapped = remap_layout_display_ids_for_snapshot(
        desired,
        &snapshot.layout,
        &raw_path_connectors(&snapshot.raw),
    );
    let current_ids: HashSet<DisplayId> = snapshot
        .layout
        .outputs
        .iter()
        .map(|output| output.display_id.clone())
        .collect();
    remapped
        .outputs
        .into_iter()
        .filter(|output| output.enabled && !current_ids.contains(&output.display_id))
        .collect()
}

/// `true` para una entrada inactiva sin geometría usable: el centinela 0x0 que
/// emite el seeder de `ALL_PATHS` para los conectados-pero-detachados, cuya
/// geometría real Windows no reporta.
fn output_is_geometry_sentinel(output: &monarch::OutputConfig) -> bool {
    !output.enabled && output.resolution.width == 0 && output.resolution.height == 0
}

fn display_is_geometry_sentinel(display: &DisplayInfo) -> bool {
    !display.is_active && display.resolution.width == 0 && display.resolution.height == 0
}

/// Fusiona el layout cacheado con el fresco.
///
/// Regla: para un conector que sigue vivo manda el dato fresco; para uno que
/// desapareció se conserva la entrada vieja marcada como apagada. Con dos
/// excepciones que son las que evitan que la UI parpadee: un EDID que se leyó mal
/// esta vez no borra el que ya sabíamos, y el centinela 0x0 no pisa la última
/// geometría real conocida.
fn merge_layout_with_fresh(
    previous: Option<&Layout>,
    fresh: &Layout,
    fresh_connectors: &HashSet<(u64, u32)>,
) -> Layout {
    let Some(previous) = previous else {
        return fresh.clone();
    };

    let mut outputs: Vec<monarch::OutputConfig> = Vec::new();
    for cached in &previous.outputs {
        if cached_id_is_stale_duplicate(
            &cached.display_id,
            fresh.outputs.iter().map(|output| &output.display_id),
            fresh_connectors,
        ) {
            continue;
        }

        let cached_connector = (cached.display_id.adapter_luid, cached.display_id.target_id);
        let next = if let Some(active) = fresh.outputs.iter().find(|active| {
            (active.display_id.adapter_luid, active.display_id.target_id) == cached_connector
        }) {
            // Mismo conector: gana el dato fresco. Si la lectura del EDID falló
            // esta vez, se conserva el hash conocido para que la identidad del
            // monitor no cambie por un tropezón momentáneo.
            let mut next = active.clone();
            if next.display_id.edid_hash.is_none() {
                next.display_id.edid_hash = cached.display_id.edid_hash;
            }
            // Una entrada sembrada inactiva no trae geometría (centinela 0x0):
            // se conserva la última geometría real de ese conector en vez de
            // borrarla en cada refresco.
            if output_is_geometry_sentinel(&next) && !output_is_geometry_sentinel(cached) {
                next.position = cached.position.clone();
                next.resolution = cached.resolution.clone();
                next.refresh_rate_mhz = cached.refresh_rate_mhz;
            }
            next
        } else {
            let mut inactive = cached.clone();
            inactive.enabled = false;
            inactive.primary = false;
            inactive
        };
        push_output_preferring_known_edid(&mut outputs, next);
    }
    for active in &fresh.outputs {
        let connector = (active.display_id.adapter_luid, active.display_id.target_id);
        if !outputs.iter().any(|output| {
            (output.display_id.adapter_luid, output.display_id.target_id) == connector
        }) {
            outputs.push(active.clone());
        }
    }

    let mut merged = Layout { outputs };
    if !merged
        .outputs
        .iter()
        .any(|output| output.enabled && output.primary)
    {
        if let Some(first) = merged.outputs.iter_mut().find(|output| output.enabled) {
            first.primary = true;
        }
    }
    merged
}

/// Inserta sin duplicar por conector, y ante un choque se queda con la versión
/// que SÍ tiene hash de EDID: es la que conserva la identidad del monitor.
fn push_output_preferring_known_edid(
    outputs: &mut Vec<monarch::OutputConfig>,
    next: monarch::OutputConfig,
) {
    let connector = (next.display_id.adapter_luid, next.display_id.target_id);
    if let Some(existing) = outputs
        .iter_mut()
        .find(|output| (output.display_id.adapter_luid, output.display_id.target_id) == connector)
    {
        if existing.display_id.edid_hash.is_none() && next.display_id.edid_hash.is_some() {
            *existing = next;
        }
        return;
    }
    outputs.push(next);
}

/// Gemelo de `merge_layout_with_fresh` para la lista de monitores. Mismas reglas;
/// al final ordena por nombre + target para que la UI no baile entre refrescos.
fn merge_displays_with_fresh(
    previous: &[DisplayInfo],
    fresh: &[DisplayInfo],
    fresh_connectors: &HashSet<(u64, u32)>,
) -> Vec<DisplayInfo> {
    let mut merged: Vec<DisplayInfo> = Vec::new();
    for cached in previous {
        if cached_id_is_stale_duplicate(
            &cached.id,
            fresh.iter().map(|display| &display.id),
            fresh_connectors,
        ) {
            continue;
        }

        let cached_connector = (cached.id.adapter_luid, cached.id.target_id);
        let next = if let Some(active) = fresh
            .iter()
            .find(|active| (active.id.adapter_luid, active.id.target_id) == cached_connector)
        {
            let mut next = active.clone();
            if next.id.edid_hash.is_none() {
                next.id.edid_hash = cached.id.edid_hash;
            }
            // Las entradas sembradas inactivas traen el centinela 0x0: se conserva
            // la última geometría real para que la UI no muestre 0x0 en cada tic.
            if display_is_geometry_sentinel(&next) && !display_is_geometry_sentinel(cached) {
                next.resolution = cached.resolution.clone();
                next.refresh_rate_mhz = cached.refresh_rate_mhz;
            }
            next
        } else {
            let mut inactive = cached.clone();
            inactive.is_active = false;
            inactive.is_primary = false;
            inactive
        };
        push_display_preferring_known_edid(&mut merged, next);
    }
    for active in fresh {
        let connector = (active.id.adapter_luid, active.id.target_id);
        if !merged
            .iter()
            .any(|display| (display.id.adapter_luid, display.id.target_id) == connector)
        {
            merged.push(active.clone());
        }
    }
    merged.sort_by(|left, right| {
        left.friendly_name
            .cmp(&right.friendly_name)
            .then(left.id.target_id.cmp(&right.id.target_id))
    });
    merged
}

fn push_display_preferring_known_edid(displays: &mut Vec<DisplayInfo>, next: DisplayInfo) {
    let connector = (next.id.adapter_luid, next.id.target_id);
    if let Some(existing) = displays
        .iter_mut()
        .find(|display| (display.id.adapter_luid, display.id.target_id) == connector)
    {
        if existing.id.edid_hash.is_none() && next.id.edid_hash.is_some() {
            *existing = next;
        }
        return;
    }
    displays.push(next);
}

impl DisplayBackend for WindowsDisplayBackend {
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ManagerError> {
        self.refresh_active()?;
        let cache = self.cache.lock().map_err(|_| cache_poisoned())?;
        Ok(cache.last_displays.clone())
    }

    fn get_layout(&self) -> Result<Layout, ManagerError> {
        self.refresh_active()?;
        let cache = self.cache.lock().map_err(|_| cache_poisoned())?;
        cache
            .last_layout
            .clone()
            .ok_or_else(|| ManagerError::Backend("no cached layout available".to_string()))
    }

    fn apply_layout(&self, layout: Layout) -> Result<(), ManagerError> {
        layout.ensure_valid()?;
        diagnostics::log(format!(
            "topology_apply:start:outputs={}",
            layout.outputs.len()
        ));

        // Se vuelve a preguntar qué está activo AHORA para que un apply que solo
        // apaga monitores trabaje sobre la base más chica posible: cuantos menos
        // paths se le pasan a Windows, menos outputs ajenos toca de prepo.
        let active_snapshot = query_active_topology()?;
        let needs_attach_paths = desired_enables_inactive_output(&layout, &active_snapshot.layout);

        let base_snapshot = if !needs_attach_paths {
            // Cambio de solo-apagar: se aplica contra un snapshot mínimo y SIN
            // enriquecer, así ningún path que vino de la base de datos de
            // Windows se le realimenta a `SetDisplayConfig` (de ahí salen los
            // errores 87 en medio de un detach).
            query_active_only_topology()?
        } else {
            // Hay que PRENDER algo: se prefiere el snapshot enriquecido que
            // guarda el cache, porque es el que todavía tiene el path del
            // monitor apagado. Solo sirve si sigue cubriendo lo que está activo.
            let cache = self.cache.lock().map_err(|_| cache_poisoned())?;

            if let Some(cached) = cache.last_snapshot.clone() {
                if raw_covers_active_outputs_raw(&cached.raw, &active_snapshot.layout) {
                    cached
                } else {
                    active_snapshot.clone()
                }
            } else {
                active_snapshot.clone()
            }
        };

        let working_layout = remap_layout_display_ids_for_snapshot(
            &layout,
            &base_snapshot.layout,
            &raw_path_connectors(&active_snapshot.raw),
        );

        let missing_attach_outputs =
            enabled_outputs_missing_from_raw(&working_layout, &base_snapshot.raw);
        let (next_snapshot, applied_layout) = if !missing_attach_outputs.is_empty() {
            // La base no tiene path para estos outputs, así que mover banderas
            // sería un no-op silencioso (`SetDisplayConfig` devuelve 0 cuando el
            // conjunto activo no cambió). Se rescata SIN CONDICIONES: una guarda
            // del tipo "¿estará conectado?" dependería de la misma enumeración
            // que acaba de no mostrar el monitor, o sea bloquearía justo el caso
            // que tiene que curar. Si el monitor de verdad no está, el costo es
            // un intento inofensivo y después el mismo error preciso.
            for output in &missing_attach_outputs {
                diagnostics::log(format!(
                    "recover:extend_attempt:{}",
                    describe_output_for_error(output, &base_snapshot)
                ));
            }
            recover_apply_with_topology_extend(
                &working_layout,
                &missing_attach_outputs,
                &active_snapshot,
            )?
        } else {
            match apply_layout_against_snapshot(&working_layout, &base_snapshot) {
                Ok(snapshot) => (snapshot, working_layout),
                Err(error) if is_set_display_invalid_parameter(&error) => {
                    diagnostics::log("topology_apply:retry:reason=setdisplayconfig_87");
                    recover_apply_with_topology_extend(&working_layout, &[], &active_snapshot)?
                }
                Err(error) => {
                    diagnostics::log(format!("topology_apply:error:{error}"));
                    return Err(error);
                }
            }
        };

        let mut cache = self.cache.lock().map_err(|_| cache_poisoned())?;
        cache.last_snapshot = Some(merge_snapshot_for_cache(
            Some(&base_snapshot),
            next_snapshot.clone(),
        ));
        merge_sdr_gamma_cache(
            &mut cache.sdr_gamma_cache,
            capture_sdr_gamma_ramps(&next_snapshot),
        );

        // El layout que queda cacheado es el que se pidió, pero con la geometría
        // que Windows terminó dando: pedir 1920x1080 y que el driver entregue
        // otra cosa es normal, y la UI tiene que mostrar lo que hay.
        let mut merged_layout = applied_layout;
        for output in &mut merged_layout.outputs {
            if let Some(active) = next_snapshot
                .layout
                .outputs
                .iter()
                .find(|active| active.display_id == output.display_id)
            {
                output.position = active.position.clone();
                output.resolution = active.resolution.clone();
                output.refresh_rate_mhz = active.refresh_rate_mhz;
                output.enabled = true;
                output.primary = active.primary;
            }
        }
        cache.last_layout = Some(merged_layout);

        let mut displays = cache.last_displays.clone();
        for display in &mut displays {
            if let Some(active) = next_snapshot.displays.iter().find(|d| d.id == display.id) {
                *display = active.clone();
            } else {
                display.is_active = false;
                display.is_primary = false;
            }
        }
        for active in &next_snapshot.displays {
            if !displays.iter().any(|d| d.id == active.id) {
                displays.push(active.clone());
            }
        }
        cache.last_displays = displays;
        drop(cache);
        diagnostics::log("topology_apply:done");

        Ok(())
    }

    fn color_state_signature(&self) -> Result<Option<String>, ManagerError> {
        WindowsDisplayBackend::color_state_signature(self)
    }

    fn reapply_color_calibration(&self) -> Result<(), ManagerError> {
        WindowsDisplayBackend::reapply_color_calibration(self)
    }

    fn invalidate_cache(&self) -> Result<(), ManagerError> {
        WindowsDisplayBackend::invalidate_cache(self)
    }

    fn prepare_attach_targets(&self, desired: &Layout) -> Result<(), ManagerError> {
        WindowsDisplayBackend::prepare_attach_targets(self, desired)
    }
}

fn merge_sdr_gamma_cache(
    cache: &mut HashMap<GammaRampKey, GammaRampWords>,
    observed: HashMap<GammaRampKey, GammaRampWords>,
) {
    for (key, ramp) in observed {
        match cache.get(&key) {
            // Si lo que se acaba de observar parece la rampa identidad (o sea,
            // un reset — pasa seguido al salir de HDR en algunos drivers) y lo
            // guardado NO lo era, se conserva lo guardado: es la calibración de
            // verdad del usuario.
            Some(existing)
                if !gamma_ramp_looks_identity(existing) && gamma_ramp_looks_identity(&ramp) => {}
            _ => {
                cache.insert(key, ramp);
            }
        }
    }
}

/// ¿El layout pedido prende algo que hoy está apagado? Es lo que separa un
/// "apagar nomás" (camino corto) de un attach (camino con red).
fn desired_enables_inactive_output(desired: &Layout, active_layout: &Layout) -> bool {
    desired.outputs.iter().any(|output| {
        output.enabled
            && !active_layout
                .outputs
                .iter()
                .any(|active| active.enabled && active.display_id == output.display_id)
    })
}

/// Reescribe los `DisplayId` de un layout guardado para que apunten a los
/// monitores de AHORA.
///
/// Hace falta porque Windows renumera las placas: después de suspender, el mismo
/// monitor físico aparece con otro `adapter_luid`. Sin este remapeo, un perfil
/// guardado ayer apunta a la nada y el apply "no encuentra" pantallas que están
/// enchufadas.
///
/// La identidad que se persigue es el hash del EDID. Sin EDID hay un plan B por
/// `target_id`, pero **nunca entre placas distintas**: iGPU y dGPU reusan la
/// misma numeración de targets, así que adivinar cruzado le pegaría al monitor
/// equivocado.
fn remap_layout_display_ids_for_snapshot(
    desired: &Layout,
    current: &Layout,
    enumerated_connectors: &HashSet<(u64, u32)>,
) -> Layout {
    let current_ids: HashSet<DisplayId> = current
        .outputs
        .iter()
        .map(|output| output.display_id.clone())
        .collect();

    if desired
        .outputs
        .iter()
        .all(|output| current_ids.contains(&output.display_id))
    {
        return desired.clone();
    }

    let mut remapped = desired.clone();
    let mut used: HashSet<DisplayId> = HashSet::new();
    for output in &remapped.outputs {
        if current_ids.contains(&output.display_id) {
            used.insert(output.display_id.clone());
        }
    }

    let mut current_by_edid: HashMap<u64, Vec<&monarch::OutputConfig>> = HashMap::new();
    for output in &current.outputs {
        if let Some(edid_hash) = output.display_id.edid_hash {
            current_by_edid.entry(edid_hash).or_default().push(output);
        }
    }

    for output in &mut remapped.outputs {
        if current_ids.contains(&output.display_id) {
            continue;
        }

        let mut replacement = None;

        if let Some(edid_hash) = output.display_id.edid_hash {
            let candidates = unique_unused_candidates(
                current_by_edid.get(&edid_hash).cloned().unwrap_or_default(),
                &used,
            );
            replacement = choose_remap_candidate(&candidates, enumerated_connectors)
                .map(|candidate| candidate.display_id.clone());
        }

        if replacement.is_none() && output.display_id.edid_hash.is_none() {
            // Plan B sin hash: nunca adivinar cruzando placas (ver el doc de
            // arriba).
            let candidates = unique_unused_candidates_by_target_id(
                output.display_id.target_id,
                &current.outputs,
                &used,
            );
            if candidates_share_one_adapter(&candidates) {
                replacement = choose_remap_candidate(&candidates, enumerated_connectors)
                    .map(|candidate| candidate.display_id.clone());
            }
        }

        if let Some(next_id) = replacement {
            used.insert(next_id.clone());
            output.display_id = next_id;
        }
    }

    remapped
}

fn candidates_share_one_adapter(candidates: &[&monarch::OutputConfig]) -> bool {
    let mut adapters = candidates
        .iter()
        .map(|candidate| candidate.display_id.adapter_luid);
    let Some(first) = adapters.next() else {
        return true;
    };
    adapters.all(|adapter| adapter == first)
}

/// Desempate determinista cuando hay más de un candidato con la misma identidad
/// (por ejemplo una entrada vieja cacheada más el mismo monitor re-enumerado con
/// otro `adapter_luid` después de despertar): primero el que Windows está
/// enumerando ahora, después el único habilitado. Dos gemelos activos e idénticos
/// quedan ambiguos a propósito y no se remapean — es preferible fallar a mover el
/// monitor equivocado.
fn choose_remap_candidate<'a>(
    candidates: &[&'a monarch::OutputConfig],
    enumerated_connectors: &HashSet<(u64, u32)>,
) -> Option<&'a monarch::OutputConfig> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return candidates.first().copied();
    }

    let enumerated: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            enumerated_connectors.contains(&(
                candidate.display_id.adapter_luid,
                candidate.display_id.target_id,
            ))
        })
        .collect();
    if enumerated.len() == 1 {
        return enumerated.first().copied();
    }

    let pool = if enumerated.is_empty() {
        candidates.to_vec()
    } else {
        enumerated
    };
    let enabled: Vec<_> = pool
        .iter()
        .copied()
        .filter(|candidate| candidate.enabled)
        .collect();
    if enabled.len() == 1 {
        return enabled.first().copied();
    }

    None
}

/// Los outputs que el layout quiere prender pero para los que el snapshot no
/// tiene ningún path. Son exactamente los que necesitan rescate: sin path, mover
/// la bandera ACTIVE no hace nada y `SetDisplayConfig` igual devuelve 0.
fn enabled_outputs_missing_from_raw<'a>(
    layout: &'a Layout,
    raw: &RawTopologySnapshot,
) -> Vec<&'a monarch::OutputConfig> {
    let connectors = raw_path_connectors(raw);
    layout
        .outputs
        .iter()
        .filter(|output| output.enabled)
        .filter(|output| {
            !connectors.contains(&(output.display_id.adapter_luid, output.display_id.target_id))
        })
        .collect()
}

/// Etiqueta legible de un output para los logs y para el mensaje de error que ve
/// el usuario.
fn describe_output_for_error(
    output: &monarch::OutputConfig,
    base_snapshot: &TopologySnapshot,
) -> String {
    let edid = output
        .display_id
        .edid_hash
        .map(|value| format!("{value:016x}"))
        .unwrap_or_else(|| "-".to_string());
    let friendly = base_snapshot
        .displays
        .iter()
        .find(|display| {
            display.id == output.display_id
                || (output.display_id.edid_hash.is_some()
                    && display.id.edid_hash == output.display_id.edid_hash)
        })
        .map(|display| format!("'{}' ", display.friendly_name))
        .unwrap_or_default();
    format!(
        "{friendly}(target_id={}, edid_hash={edid})",
        output.display_id.target_id
    )
}

const RECOVER_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(3500);
const RECOVER_SETTLE_STEP: std::time::Duration = std::time::Duration::from_millis(250);
/// Ventana de gracia después de un attach explícito que Windows YA aceptó: solo
/// tiene que cubrir el apretón de manos del monitor, así que es mucho más corta
/// que el plazo de un extend, que puede tener que despertar un target de cero.
const ATTACH_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(1500);
/// Tope de vueltas del sondeo, aparte del plazo por reloj.
///
/// El plazo ya acota el lazo (`Instant` es monotónico), así que esto es cinturón
/// además de tiradores — pero es gratis y la regla del proyecto es que **ningún
/// bucle de reintento queda sin contador**: esto corre en un hilo bloqueante, y
/// un hilo colgado para siempre se lleva puesto un worker del pool.
/// Generoso a propósito: el peor caso legítimo es
/// `RECOVER_SETTLE_DEADLINE / RECOVER_SETTLE_STEP` = 14 vueltas.
const MAX_SETTLE_ATTEMPTS: usize = 64;

/// Rellena la geometría de los outputs habilitados que todavía traen el centinela
/// 0x0 (un monitor sembrado desde `ALL_PATHS` que nunca estuvo activo en este
/// arranque) usando el snapshot post-rescate, donde Windows ya le asignó un modo
/// de verdad.
fn fill_sentinel_geometry_from_snapshot(layout: &mut Layout, snapshot: &TopologySnapshot) {
    for output in &mut layout.outputs {
        if !output.enabled || output.resolution.width != 0 || output.resolution.height != 0 {
            continue;
        }
        let Some(active) = snapshot
            .layout
            .outputs
            .iter()
            .find(|active| active.display_id == output.display_id)
        else {
            continue;
        };
        output.position = active.position.clone();
        output.resolution = active.resolution.clone();
        output.refresh_rate_mhz = active.refresh_rate_mhz;
    }
}

/// Los sources `(adapter luid del source, source id)` que hoy manejan un path
/// activo. Prender un target sobre un source ocupado **clonaría** esa pantalla en
/// vez de extender el escritorio.
fn active_source_keys(snapshot: &TopologySnapshot) -> HashSet<(u64, u32)> {
    snapshot
        .raw
        .paths
        .iter()
        .filter(|path| path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .map(|path| {
            (
                luid_to_u64(
                    path.sourceInfo.adapterId.HighPart,
                    path.sourceInfo.adapterId.LowPart,
                ),
                path.sourceInfo.id,
            )
        })
        .collect()
}

fn attachable_source_key(candidate: &AttachablePath) -> (u64, u32) {
    (
        luid_to_u64(
            candidate.path.sourceInfo.adapterId.HighPart,
            candidate.path.sourceInfo.adapterId.LowPart,
        ),
        candidate.path.sourceInfo.id,
    )
}

/// Candidatos de attach para `display_id` cuyo source esté libre. `ALL_PATHS`
/// reporta una entrada por combinación (source, target); las que tienen el source
/// ocupado se descartan —clonarían en vez de extender— en lugar de reescribirlas.
fn select_attach_candidates<'a>(
    attachable: &'a [AttachablePath],
    display_id: &DisplayId,
    used_source_keys: &HashSet<(u64, u32)>,
) -> Vec<&'a AttachablePath> {
    attachable
        .iter()
        .filter(|candidate| {
            candidate.adapter_luid == display_id.adapter_luid
                && candidate.target_id == display_id.target_id
        })
        .filter(|candidate| !used_source_keys.contains(&attachable_source_key(candidate)))
        .collect()
}

/// Prende TODOS los outputs que faltan en UNA sola llamada a `SetDisplayConfig`.
///
/// El array de paths que se entrega es la topología completa, así que una llamada
/// por monitor apagaría lo que prendió la anterior. En vez de eso el lote crece
/// de a un candidato, cada paso confirmado con una sonda `SDC_VALIDATE` gratis
/// (si un candidato rebota se prueban los otros sources), y al final va un solo
/// apply — un cambio de topología, no N.
///
/// Devuelve `true` solo si el apply final dio 0. Eso **igual NO prueba** que
/// ningún monitor haya vuelto (`SetDisplayConfig` devuelve 0 para un no-op), así
/// que el que llama tiene que confirmarlo contra una enumeración fresca.
fn try_batch_explicit_attach(
    missing: &[&monarch::OutputConfig],
    active_snapshot: &TopologySnapshot,
) -> bool {
    // Guarda: el camino de recuperación del error 87 entra acá sin nada faltante.
    // Sin esto, un lote vacío informaría "está todo prendido" y mataría en
    // silencio el fallback del extend.
    if missing.is_empty() {
        return false;
    }

    let mut used_source_keys = active_source_keys(active_snapshot);
    let mut batch: Vec<&AttachablePath> = Vec::new();

    for output in missing {
        let description = describe_output_for_error(output, active_snapshot);
        let candidates = select_attach_candidates(
            &active_snapshot.attachable,
            &output.display_id,
            &used_source_keys,
        );
        if candidates.is_empty() {
            diagnostics::log(format!("recover:no_attachable_candidate:{description}"));
            continue;
        }

        let mut accepted = false;
        for candidate in candidates {
            batch.push(candidate);
            let paths = build_attach_paths(&batch, active_snapshot);
            let status = validate_attach_paths(&paths, active_snapshot);
            diagnostics::log(format!(
                "recover:explicit_attach:{description}:source={}:validate={status}",
                attachable_source_key(candidate).1
            ));
            if status == 0 {
                // Se reserva el source para que un output posterior del mismo
                // lote no lo reuse.
                used_source_keys.insert(attachable_source_key(candidate));
                accepted = true;
                break;
            }
            batch.pop();
        }
        if !accepted {
            diagnostics::log(format!(
                "recover:explicit_attach:{description}:no_candidate_validated"
            ));
        }
    }

    if batch.is_empty() {
        return false;
    }

    let paths = build_attach_paths(&batch, active_snapshot);
    let status = apply_attach_paths(&paths, active_snapshot);
    diagnostics::log(format!(
        "recover:explicit_attach:batch={}:apply={status}",
        batch.len()
    ));
    status == 0
}

/// Deshace, con la mejor voluntad, un rescate que no funcionó.
///
/// Tanto el attach explícito como el extend cambian (y persisten) la topología,
/// así que dejarlos puestos sería reescribirle la configuración al usuario por un
/// attach fallido. Re-aplicar el layout previo alcanza porque su conjunto de
/// habilitados son exactamente los que estaban activos antes, y el `unwrap_or(false)`
/// del apply apaga todo lo que el rescate agregó.
fn restore_pre_extend_topology(pre_extend: &TopologySnapshot) {
    match apply_layout_against_snapshot(&pre_extend.layout, pre_extend) {
        Ok(_) => diagnostics::log("recover:restore_ok"),
        Err(error) => diagnostics::log(format!("recover:restore_failed:{error}")),
    }
}

/// La topología previa es la ÚNICA red de rollback en una máquina sin panel
/// interno, así que es una **precondición dura**, no un extra opcional:
/// se captura (con un reintento, porque falla justo cuando es más probable un
/// tropezón momentáneo de `QueryDisplayConfig`) o no se toca la topología.
///
/// El tipo de retorno es `Result`, no `Option`, a propósito: así el "saltearlo en
/// silencio" es imposible por construcción (Monarch ADR-009).
fn capture_pre_recovery_state() -> Result<TopologySnapshot, ManagerError> {
    match query_active_only_topology() {
        Ok(snapshot) => Ok(snapshot),
        Err(first_error) => {
            diagnostics::log(format!(
                "recover:pre_state_query_failed:{first_error}:retrying"
            ));
            std::thread::sleep(RECOVER_SETTLE_STEP);
            query_active_only_topology()
        }
    }
}

enum SettleOutcome {
    Settled(TopologySnapshot, Layout),
    StillMissing(String),
}

/// Sondea una enumeración fresca hasta que todos los outputs habilitados de
/// `working_layout` resuelvan, o hasta que se venza el plazo.
///
/// Sondear (en vez de dormir una vez y creer) es lo que necesita el apretón de
/// manos de un HDMI o una TV, y el remapeo se rehace en cada vuelta porque el
/// conector puede volver con otro `(adapter_luid, target_id)`.
///
/// Informa lo que observó y nada más: el rollback y la redacción del error los
/// decide el que llama.
fn settle_poll(
    working_layout: &Layout,
    deadline: std::time::Duration,
    label: &str,
) -> Result<SettleOutcome, ManagerError> {
    let deadline_at = std::time::Instant::now() + deadline;
    let mut attempt = 0usize;
    loop {
        attempt += 1;
        std::thread::sleep(RECOVER_SETTLE_STEP);
        let snapshot = query_active_topology()?;
        let layout = remap_layout_display_ids_for_snapshot(
            working_layout,
            &snapshot.layout,
            &raw_path_connectors(&snapshot.raw),
        );
        let missing = enabled_outputs_missing_from_raw(&layout, &snapshot.raw);
        diagnostics::log(format!(
            "recover:settle_poll:{label}:{attempt}:missing={}",
            missing.len()
        ));
        if missing.is_empty() {
            return Ok(SettleOutcome::Settled(snapshot, layout));
        }
        if std::time::Instant::now() >= deadline_at || attempt >= MAX_SETTLE_ATTEMPTS {
            // `first()` en vez de `[0]`: el `is_empty` de arriba ya lo garantiza,
            // pero un indexado crudo que se equivoque acá aborta el proceso.
            let description = missing
                .first()
                .map(|output| describe_output_for_error(output, &snapshot))
                .unwrap_or_else(|| "(sin detalle)".to_string());
            return Ok(SettleOutcome::StillMissing(description));
        }
    }
}

/// Aplica el layout pedido una vez que el rescate trajo todo de vuelta. Si ese
/// apply final falla, se revierte a la topología previa antes de devolver el error.
fn finish_recovery(
    recovered_snapshot: TopologySnapshot,
    retry_layout: Layout,
    pre_state: &TopologySnapshot,
) -> Result<(TopologySnapshot, Layout), ManagerError> {
    let mut retry_layout = retry_layout;
    fill_sentinel_geometry_from_snapshot(&mut retry_layout, &recovered_snapshot);
    match apply_layout_against_snapshot(&retry_layout, &recovered_snapshot) {
        Ok(snapshot) => {
            diagnostics::log("recover:retry_result:ok");
            Ok((snapshot, retry_layout))
        }
        Err(error) => {
            diagnostics::log(format!("recover:retry_result:{error}"));
            restore_pre_extend_topology(pre_state);
            Err(error)
        }
    }
}

/// **La escalera de rescate.** Cuatro escalones, en orden, y cada uno se juzga
/// re-enumerando, nunca por el código de retorno de Windows.
fn recover_apply_with_topology_extend(
    working_layout: &Layout,
    missing: &[&monarch::OutputConfig],
    active_snapshot: &TopologySnapshot,
) -> Result<(TopologySnapshot, Layout), ManagerError> {
    // La red de rollback es precondición dura: nunca se toca la topología sin ella.
    let pre_state = match capture_pre_recovery_state() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            diagnostics::log("recover:abort:no_pre_state_captured");
            return Err(error);
        }
    };

    // Cada escalón que de verdad se intentó, para que el error final los pueda
    // nombrar con honestidad.
    let mut attempted: Vec<&str> = Vec::new();

    // (a) Attach explícito: prende estos targets exactos desde sus propios paths
    // enumerados, igual que hace el panel de Configuración de Windows.
    // `SDC_TOPOLOGY_EXTEND` no puede reemplazarlo — reproduce la última
    // configuración extendida de la base de persistencia, y un detach nuestro
    // (guardado con `SDC_SAVE_TO_DATABASE`) ya sacó a este monitor de esa entrada.
    if try_batch_explicit_attach(missing, active_snapshot) {
        attempted.push("an explicit attach");
        // Un 0 de `SetDisplayConfig` solo significa "aceptado", nunca "el monitor
        // volvió": se confirma contra una enumeración fresca y, si no volvió, se
        // sigue escalando.
        match settle_poll(working_layout, ATTACH_SETTLE_DEADLINE, "attach") {
            Ok(SettleOutcome::Settled(snapshot, layout)) => {
                diagnostics::log("recover:resolved:explicit_attach");
                return finish_recovery(snapshot, layout, &pre_state);
            }
            Ok(SettleOutcome::StillMissing(_)) => {
                diagnostics::log("recover:attach_not_observed:escalating");
            }
            Err(error) => {
                restore_pre_extend_topology(&pre_state);
                return Err(error);
            }
        }
    }

    // (b) Topology extend por CCD. Su status tampoco puede juzgar el éxito (el 0
    // también sale para un no-op), así que decide el sondeo.
    attempted.push("a topology extend");
    try_topology_extend();
    let still_missing = match settle_poll(working_layout, RECOVER_SETTLE_DEADLINE, "extend") {
        Ok(SettleOutcome::Settled(snapshot, layout)) => {
            diagnostics::log("recover:resolved:topology_extend");
            return finish_recovery(snapshot, layout, &pre_state);
        }
        Ok(SettleOutcome::StillMissing(description)) => description,
        Err(error) => {
            restore_pre_extend_topology(&pre_state);
            return Err(error);
        }
    };

    // (c) DisplaySwitch: el mismo camino del shell que usa Win+P. Último recurso.
    diagnostics::log(format!("recover:escalate:display_switch:{still_missing}"));
    if let Err(error) = run_display_switch_extend() {
        diagnostics::log(format!("recover:display_switch_failed:{error}"));
        restore_pre_extend_topology(&pre_state);
        return Err(error);
    }
    attempted.push("DisplaySwitch /extend");

    let still_missing = match settle_poll(working_layout, RECOVER_SETTLE_DEADLINE, "display_switch")
    {
        Ok(SettleOutcome::Settled(snapshot, layout)) => {
            diagnostics::log("recover:resolved:display_switch");
            return finish_recovery(snapshot, layout, &pre_state);
        }
        Ok(SettleOutcome::StillMissing(description)) => description,
        Err(error) => {
            restore_pre_extend_topology(&pre_state);
            return Err(error);
        }
    };

    // (d) Se acabaron las opciones: se deshace todo lo que el rescate tocó y se
    // dice exactamente qué se probó.
    diagnostics::log(format!("recover:still_missing:{still_missing}"));
    restore_pre_extend_topology(&pre_state);
    Err(ManagerError::Backend(format!(
        "cannot attach display {still_missing}: it did not come back after {}. reconnect it or attach it once from Windows Display settings",
        attempted.join(", then ")
    )))
}

fn unique_unused_candidates<'a>(
    candidates: Vec<&'a monarch::OutputConfig>,
    used: &HashSet<DisplayId>,
) -> Vec<&'a monarch::OutputConfig> {
    candidates
        .into_iter()
        .filter(|candidate| !used.contains(&candidate.display_id))
        .collect()
}

fn unique_unused_candidates_by_target_id<'a>(
    target_id: u32,
    current_outputs: &'a [monarch::OutputConfig],
    used: &HashSet<DisplayId>,
) -> Vec<&'a monarch::OutputConfig> {
    current_outputs
        .iter()
        .filter(|candidate| candidate.display_id.target_id == target_id)
        .filter(|candidate| !used.contains(&candidate.display_id))
        .collect()
}

/// El error 87 (`ERROR_INVALID_PARAMETER`) de `SetDisplayConfig` es el que
/// dispara el rescate en vez de fallar de una: casi siempre es una combinación de
/// paths que Windows no digiere, no un monitor ausente.
///
/// Se reconoce por el texto porque `ManagerError` no lleva el código de Windows
/// aparte. **La cadena no se escribe acá**: la arma y la reconoce `apply.rs`, que
/// es quien produce el error. Antes estaba duplicada a mano en los dos archivos,
/// y cambiar el `format!` de un lado apagaba este rescate en silencio.
fn is_set_display_invalid_parameter(error: &ManagerError) -> bool {
    super::apply::is_invalid_parameter_error(error)
}
