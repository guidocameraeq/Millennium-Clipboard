// El motor que efectivamente cambia los monitores. Migrado de Monarch @ 7f9f63b
// (`src-tauri/src/backend/windows/apply.rs`) — ver docs/DECISIONS.md ADR-002 y la
// sección "Doctrina CCD heredada".
//
// FASE 2 del SPEC-displays. Este es el archivo que la Fase 1 dejó a propósito
// fuera del repo: acá viven las cinco llamadas a `SetDisplayConfig`. Todo lo que
// se ejecuta desde acá le toca las pantallas al usuario de verdad.
//
// LOS COMENTARIOS DE ESTE ARCHIVO SON EL ACTIVO, no la decoración. Cada tabla de
// flags de abajo salió de sondear la API con `SDC_VALIDATE` contra hardware real,
// y contradice a la documentación de Microsoft en más de un punto. Costó meses.
// Si alguna vez hay que "simplificar" una combinación de flags, la respuesta es
// no: primero se vuelve a sondear.
//
// Dos reglas que atraviesan todo el archivo:
//   1. Un status de retorno NUNCA prueba que algo pasó. `SetDisplayConfig`
//      devuelve 0 para un no-op documentado. La única prueba es re-enumerar.
//   2. Nada de acá puede entrar en pánico: Millennium compila release con
//      `panic = "abort"`, así que un panic en el módulo de monitores se lleva
//      puesto el portapapeles, el discovery y las transferencias.
#![cfg(target_os = "windows")]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use super::diagnostics;
use monarch::{Layout, ManagerError};
use windows::core::BOOL;
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, SetDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_NO_OPTIMIZATION,
    SDC_PATH_PERSIST_IF_REQUIRED, SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VALIDATE,
};
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::ColorSystem::{
    GetDeviceGammaRamp, SetDeviceGammaRamp, WcsGetCalibrationManagementState,
    WcsSetCalibrationManagementState,
};
use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper, DESKTOP_WALLPAPER_POSITION};

use super::win32_types::{luid_to_u64, AttachablePath, TopologySnapshot};

const DISPLAYCONFIG_PATH_ACTIVE_FLAG: u32 = 0x0000_0001;
/// `DISPLAYCONFIG_PATH_MODE_IDX_INVALID`: "no te paso modo, elegilo vos, Windows".
const DISPLAYCONFIG_PATH_MODE_IDX_INVALID: u32 = 0xffff_ffff;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GAMMA_RAMP_WORDS: usize = 3 * 256;
pub(super) type GammaRampKey = (u64, u32);
pub(super) type GammaRampWords = [u16; GAMMA_RAMP_WORDS];

pub fn apply_layout_against_snapshot(
    desired: &Layout,
    snapshot: &TopologySnapshot,
) -> Result<TopologySnapshot, ManagerError> {
    desired.ensure_valid()?;
    let saved_gamma_ramps = capture_active_gamma_ramps(snapshot);
    let saved_wallpapers = capture_active_wallpapers(snapshot);
    let saved_wallpaper_position = capture_wallpaper_position();

    let desired_outputs = desired_output_index(desired);
    let mut next_paths: Vec<DISPLAYCONFIG_PATH_INFO> = snapshot.raw.paths.clone();
    let mut next_modes: Vec<DISPLAYCONFIG_MODE_INFO> = snapshot.raw.modes.clone();
    for path in &mut next_paths {
        let key = path_target_key(path);
        let desired_output = desired_outputs.get(&key);
        let enabled = desired_output.map(|output| output.enabled).unwrap_or(false);

        if enabled {
            path.flags |= DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        } else {
            path.flags &= !DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        }

        if enabled {
            apply_desired_source_mode(path, &mut next_modes, desired_output);
            apply_desired_target_refresh(path, desired_output);
        }
    }
    reorder_paths_for_desired_priority(&mut next_paths, &desired_outputs);

    unsafe {
        // Primero se intenta el apply EXACTO, para minimizar los ajustes
        // "serviciales" de topología y de modo que hace Windows y que terminan
        // moviéndole la geometría a los monitores que ni se tocaron. Recién si
        // ese rebota se cae a ALLOW_CHANGES, que es más permisivo pero deja que
        // Windows decida.
        let exact_flags = SDC_APPLY
            | SDC_USE_SUPPLIED_DISPLAY_CONFIG
            | SDC_SAVE_TO_DATABASE
            | SDC_NO_OPTIMIZATION;
        let mut status = SetDisplayConfig(
            Some(next_paths.as_slice()),
            Some(next_modes.as_slice()),
            exact_flags,
        );
        if status != 0 {
            diagnostics::log(format!("apply:sdc_failed:{status}:exact_flags"));
            status = SetDisplayConfig(
                Some(next_paths.as_slice()),
                Some(next_modes.as_slice()),
                SDC_APPLY
                    | SDC_USE_SUPPLIED_DISPLAY_CONFIG
                    | SDC_SAVE_TO_DATABASE
                    | SDC_ALLOW_CHANGES,
            );
        }

        if status != 0 {
            diagnostics::log(format!("apply:sdc_failed:{status}:allow_changes"));
            return Err(set_display_config_error(status));
        }
    }

    // Re-enumerar NO es cosmético: el status de arriba puede ser 0 sin que haya
    // cambiado nada. Esta foto nueva es la única evidencia de qué quedó.
    let next_snapshot = super::enumerate::query_active_topology()?;
    best_effort_reload_color_calibration();
    best_effort_restore_gamma_ramps(&next_snapshot, &saved_gamma_ramps);
    best_effort_restore_wallpapers(&next_snapshot, &saved_wallpapers);
    best_effort_restore_wallpaper_position(saved_wallpaper_position);
    Ok(next_snapshot)
}

/// Arma el array de paths que activa a `candidates` **encima** de los paths que
/// ya están activos: los activos conservan sus índices de modo (así los otros
/// monitores mantienen su geometría exacta) y cada candidato se agrega con el
/// flag ACTIVE y sin índices de modo, para que Windows le calcule el suyo. Es lo
/// mismo que hace el panel de Configuración de pantalla de Windows, y es la cura
/// del caso que `SDC_TOPOLOGY_EXTEND` NO puede arreglar: el extend reproduce la
/// última configuración extendida guardada en la base de persistencia, y un
/// detach hecho por Monarch (guardado con `SDC_SAVE_TO_DATABASE`) ya le sacó ese
/// monitor de ahí.
///
/// El array es la topología **COMPLETA** (`SDC_USE_SUPPLIED_DISPLAY_CONFIG`):
/// cualquier path que quede afuera se desactiva. Por eso TODOS los candidatos
/// tienen que ir en un solo array — attachearlos de a uno desconectaría lo que
/// attacheó la llamada anterior.
///
/// Lo que las sondas `SDC_VALIDATE` dejaron establecido (en una máquina de un
/// solo monitor):
///   paths activos + array de modos, un path con índices de modo inválidos -> aceptado
///   todos los paths con índices inválidos + array de modos en NULL        -> 87, siempre
/// o sea: la forma "sin modos" es un rechazo a nivel de parámetros y ni se
/// intenta.
///
/// Agregar el path de un target hoy INACTIVO —que es exactamente lo que hace el
/// bucle de abajo— no se pudo sondear ahí (esa máquina no tiene ningún target
/// conectado-pero-inactivo), pero un log de campo lo confirmó después en hardware
/// real: una TV desconectada antes de reiniciar la app, en un escritorio de 3
/// monitores, volvió en el primer poll.
///   recover:explicit_attach:'Smart TV Pro' (target_id=4352, ...):source=2:validate=0
///   recover:explicit_attach:batch=1:apply=0
///   recover:settle_poll:attach:1:missing=0
/// Igual, la sonda `SDC_VALIDATE` obligatoria antes de cada apply sigue filtrando
/// cada intento en runtime: que una máquina esté de acuerdo no significa que
/// todos los drivers lo estén.
///
/// Devuelve un vec vacío cuando no hay paths activos sobre los cuales construir.
pub(super) fn build_attach_paths(
    candidates: &[&AttachablePath],
    active_snapshot: &TopologySnapshot,
) -> Vec<DISPLAYCONFIG_PATH_INFO> {
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = active_snapshot
        .raw
        .paths
        .iter()
        .filter(|path| path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .copied()
        .collect();
    if paths.is_empty() {
        return Vec::new();
    }

    for candidate in candidates {
        let mut next = candidate.path;
        next.flags |= DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        // El donante envolvía esto en `unsafe`; en `windows 0.60` ESCRIBIR un
        // campo de union ya no lo necesita (leerlo sí, y esos `unsafe` siguen
        // donde estaban). Se saca el bloque de más para que la advertencia no
        // tape una de verdad.
        next.sourceInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        next.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        paths.push(next);
    }
    paths
}

/// Acá `SDC_ALLOW_CHANGES` SÍ es legal (y hace falta, para que Windows pueda
/// calcular el modo nuevo): solo se lo rechaza cuando va acompañado de algún
/// `SDC_TOPOLOGY_*`.
/// Prefijo del mensaje de error de un `SetDisplayConfig` fallido.
///
/// Existe como constante porque `ManagerError` no lleva el código de Windows
/// aparte, así que el rescate del **error 87** (`topology::is_set_display_invalid_parameter`)
/// tiene que reconocerlo por el texto. Con las dos puntas leyendo de acá, cambiar
/// el formato ya no puede apagar ese rescate en silencio.
pub(super) const SET_DISPLAY_CONFIG_FAILED: &str = "SetDisplayConfig failed: ";

/// Código de Windows para "parámetro inválido". Es el que dispara la escalera de
/// rescate: aparece cuando se le realimenta a `SetDisplayConfig` una combinación
/// de paths que no puede resolver.
pub(super) const ERROR_INVALID_PARAMETER: i32 = 87;

/// El error de un `SetDisplayConfig` que falló, con su status crudo adentro.
pub(super) fn set_display_config_error(status: i32) -> ManagerError {
    ManagerError::Backend(format!("{SET_DISPLAY_CONFIG_FAILED}{status}"))
}

/// `true` si este error es el 87 que la escalera de rescate sabe curar.
pub(super) fn is_invalid_parameter_error(error: &ManagerError) -> bool {
    matches!(
        error,
        ManagerError::Backend(message)
            if message.contains(&format!("{SET_DISPLAY_CONFIG_FAILED}{ERROR_INVALID_PARAMETER}"))
    )
}

fn attach_flags() -> windows::Win32::Devices::Display::SET_DISPLAY_CONFIG_FLAGS {
    SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES
}

/// Ensayo en seco: `SDC_VALIDATE` no cambia nada, así que llamarlo es gratis y es
/// **obligatorio** antes de cualquier apply — esto corre en escritorios sin panel
/// interno, donde un apply malo no deja ninguna pantalla de rescate.
/// Devuelve el status crudo de `SetDisplayConfig` (0 = la configuración se acepta).
pub(super) fn validate_attach_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    active_snapshot: &TopologySnapshot,
) -> i32 {
    if paths.is_empty() {
        return -1;
    }
    unsafe {
        SetDisplayConfig(
            Some(paths),
            Some(active_snapshot.raw.modes.as_slice()),
            SDC_VALIDATE | attach_flags(),
        )
    }
}

/// Aplica un array de paths que `validate_attach_paths` ya aceptó. Devuelve el
/// status crudo.
///
/// OJO: un 0 acá **NO prueba** que ningún monitor haya vuelto — `SetDisplayConfig`
/// devuelve 0 para un no-op sobre un conjunto activo que no cambió. El que llama
/// tiene que confirmarlo contra una enumeración fresca.
pub(super) fn apply_attach_paths(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    active_snapshot: &TopologySnapshot,
) -> i32 {
    if paths.is_empty() {
        return -1;
    }
    unsafe {
        SetDisplayConfig(
            Some(paths),
            Some(active_snapshot.raw.modes.as_slice()),
            SDC_APPLY | attach_flags(),
        )
    }
}

/// Le pide a Windows que reproduzca la última configuración extendida que tiene
/// guardada en su base de persistencia.
///
/// Devuelve el status crudo de `SetDisplayConfig` y **siempre** lo loguea —
/// incluido el 0, que NO significa que el monitor haya vuelto: cuando la entrada
/// guardada ya coincide con la topología actual, esto es un no-op que sale bien.
/// Solo el que llama sabe qué target está persiguiendo, así que solo él puede
/// juzgar el éxito, mirando una enumeración fresca.
///
/// Combinación de flags verificada empíricamente con sondas `SDC_VALIDATE` (la
/// afirmación de MSDN de que "`SDC_ALLOW_CHANGES` se permite con cualquier otra
/// combinación válida" es **FALSA**):
///   EXTEND|ALLOW_CHANGES|SAVE_TO_DATABASE -> 87   (esto es lo que este código mandaba, siempre)
///   EXTEND|ALLOW_CHANGES|PERSIST          -> 87
///   EXTEND|ALLOW_CHANGES                  -> 87
///   EXTEND|PERSIST                        -> flags aceptados
///   EXTEND                                -> flags aceptados
///   CLONE|ALLOW_CHANGES -> 87  vs  CLONE  -> flags aceptados
/// O sea: `SDC_ALLOW_CHANGES` es **ilegal** junto a cualquier `SDC_TOPOLOGY_*`, y
/// `SDC_SAVE_TO_DATABASE` exige `SDC_USE_SUPPLIED_DISPLAY_CONFIG` (esto sí está
/// documentado), que un `TOPOLOGY_*` no puede llevar.
/// `SDC_PATH_PERSIST_IF_REQUIRED` importa acá: un detach por CCD le borra al
/// target la persistencia de su path, y sin este flag el extend se saltearía ese
/// monitor.
pub(super) fn try_topology_extend() -> i32 {
    let status = unsafe {
        SetDisplayConfig(
            None,
            None,
            SDC_APPLY | SDC_TOPOLOGY_EXTEND | SDC_PATH_PERSIST_IF_REQUIRED,
        )
    };
    diagnostics::log(format!("apply:sdc_status:{status}:topology_extend"));
    status
}

/// Maneja el mismo camino del shell que usa Win+P. Es la escalada de último
/// recurso, y la decide el que llama, cuando el extend por CCD no trajo de vuelta
/// el monitor.
pub(super) fn run_display_switch_extend() -> Result<(), ManagerError> {
    let display_switch_child = Command::new("DisplaySwitch.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .arg("/extend")
        .spawn()
        .map_err(|err| {
            ManagerError::Backend(format!("DisplaySwitch /extend launch failed: {err}"))
        })?;

    let Some(display_switch_status) = wait_child_with_timeout(
        display_switch_child,
        "DisplaySwitch.exe",
        Duration::from_secs(10),
    ) else {
        return Err(ManagerError::Backend(
            "DisplaySwitch /extend timed out".to_string(),
        ));
    };

    if !display_switch_status.success() {
        return Err(ManagerError::Backend(format!(
            "DisplaySwitch /extend failed with exit code {:?}",
            display_switch_status.code()
        )));
    }

    Ok(())
}

pub(super) fn reapply_color_calibration_for_active_with_cached_sdr(
    cached_sdr_ramps: &HashMap<GammaRampKey, GammaRampWords>,
) -> Result<(), ManagerError> {
    best_effort_reload_color_calibration();
    let refreshed_snapshot = super::enumerate::query_active_topology()?;
    best_effort_restore_gamma_ramps(&refreshed_snapshot, cached_sdr_ramps);
    Ok(())
}

pub(super) fn capture_sdr_gamma_ramps(
    snapshot: &TopologySnapshot,
) -> HashMap<GammaRampKey, GammaRampWords> {
    let mut ramps = HashMap::new();

    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        // Los monitores en HDR quedan afuera: su rampa no es la curva SDR y
        // restaurársela después dejaría la pantalla con los colores lavados.
        if target_advanced_color_enabled(path).unwrap_or(false) {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );

        let Some(device_name) = source_gdi_device_name(path) else {
            continue;
        };
        let Some(ramp) = get_gamma_ramp_for_device(&device_name) else {
            continue;
        };
        ramps.insert(key, ramp);
    }

    ramps
}

pub(super) fn gamma_ramp_looks_identity(ramp: &GammaRampWords) -> bool {
    // La rampa identidad es aproximadamente i * 257 en cada canal. Se tolera algo
    // de ruido de cuantización del driver.
    //
    // El indexado es seguro por construcción (`GAMMA_RAMP_WORDS == 3 * 256`, y los
    // dos bucles no salen de ahí), pero igual va por `.get()`: con
    // `panic = "abort"` un índice fuera de rango no rompería solo esta función,
    // mataría el proceso entero. El costo es una comparación por palabra.
    let tolerance = 384u16;
    for channel in 0..3 {
        let base = channel * 256;
        for i in 0..256usize {
            let expected = (i as u32 * 257) as i32;
            let Some(&word) = ramp.get(base + i) else {
                return false;
            };
            let actual = word as i32;
            if (actual - expected).unsigned_abs() > tolerance as u32 {
                return false;
            }
        }
    }
    true
}

pub(super) fn active_color_state_signature(snapshot: &TopologySnapshot) -> String {
    let mut entries: Vec<(u64, u32, Option<bool>)> = Vec::new();

    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );
        entries.push((key.0, key.1, target_advanced_color_enabled(path)));
    }

    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut signature = String::new();
    for (index, (adapter_luid, target_id, hdr_enabled)) in entries.iter().enumerate() {
        if index > 0 {
            signature.push(';');
        }
        let hdr_flag = match hdr_enabled {
            Some(true) => '1',
            Some(false) => '0',
            None => 'x',
        };
        signature.push_str(&format!("{adapter_luid:016x}:{target_id}:{hdr_flag}"));
    }

    signature
}

fn best_effort_reload_color_calibration() {
    if std::env::var_os("MONARCH_SKIP_COLOR_RELOAD").is_some() {
        return;
    }

    // Cambiar la topología le resetea la calibración de gamma/LUT a algunos
    // drivers. Primero se prueba el camino de usuario: prender y apagar la
    // gestión de calibración de WCS (off->on) para forzar la recalibración sin
    // pedir permisos de administrador.
    unsafe {
        let mut enabled = BOOL(0);
        if WcsGetCalibrationManagementState(&mut enabled).as_bool() && enabled.as_bool() {
            let disabled = WcsSetCalibrationManagementState(false);
            let reenabled = WcsSetCalibrationManagementState(true);
            if disabled.as_bool() && reenabled.as_bool() {
                return;
            }
        }
    }

    // Plan B: disparar la tarea programada de Windows que carga la calibración.
    // Puede fallar por permisos de usuario estándar en algunas máquinas; no
    // importa, es best-effort.
    if let Ok(child) = Command::new("schtasks.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "/Run",
            "/TN",
            r"\Microsoft\Windows\WindowsColorSystem\Calibration Loader",
        ])
        .spawn()
    {
        let _ = wait_child_with_timeout(child, "schtasks.exe", Duration::from_secs(5));
    }
}

/// Sondea un proceso hijo cada 100 ms hasta que termine o se venza el timeout. Si
/// se vence, se lo mata y se devuelve `None`, así un proceso auxiliar colgado
/// nunca puede bloquear un apply para siempre (y con él, el mutex del estado
/// global).
fn wait_child_with_timeout(mut child: Child, name: &str, timeout: Duration) -> Option<ExitStatus> {
    let poll_step = Duration::from_millis(100);
    let deadline = Instant::now() + timeout;
    // El lazo está acotado por `deadline` en todos sus caminos: sale por proceso
    // terminado, por error al sondear, o por vencimiento — y en los dos últimos
    // casos mata al hijo antes de volver. No hay salida sin tope.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(err) => {
                diagnostics::log(format!("child_wait:error:{name}:{err}"));
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
        if Instant::now() >= deadline {
            diagnostics::log(format!("child_wait:timeout:{name}"));
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(poll_step);
    }
}

fn capture_active_gamma_ramps(snapshot: &TopologySnapshot) -> HashMap<(u64, u32), GammaRampWords> {
    let mut ramps = HashMap::new();

    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );

        let Some(device_name) = source_gdi_device_name(path) else {
            continue;
        };
        let Some(ramp) = get_gamma_ramp_for_device(&device_name) else {
            continue;
        };
        ramps.insert(key, ramp);
    }

    ramps
}

fn capture_active_wallpapers(snapshot: &TopologySnapshot) -> HashMap<(u64, u32), String> {
    let Some(session) = create_desktop_wallpaper_session() else {
        return HashMap::new();
    };
    let mut wallpapers = HashMap::new();

    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );

        let Some(monitor_device_path) = target_monitor_device_path(path) else {
            continue;
        };
        let Some(wallpaper_path) =
            get_wallpaper_for_monitor(&session.desktop_wallpaper, &monitor_device_path)
        else {
            continue;
        };
        wallpapers.insert(key, wallpaper_path);
    }

    wallpapers
}

fn best_effort_restore_gamma_ramps(
    snapshot: &TopologySnapshot,
    ramps: &HashMap<(u64, u32), GammaRampWords>,
) {
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );

        let Some(ramp) = ramps.get(&key) else {
            continue;
        };
        let Some(device_name) = source_gdi_device_name(path) else {
            continue;
        };
        let _ = set_gamma_ramp_for_device(&device_name, ramp);
    }
}

fn best_effort_restore_wallpapers(
    snapshot: &TopologySnapshot,
    wallpapers: &HashMap<(u64, u32), String>,
) {
    if wallpapers.is_empty() {
        return;
    }

    let Some(session) = create_desktop_wallpaper_session() else {
        return;
    };

    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let key = (
            luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            ),
            path.targetInfo.id,
        );
        let Some(wallpaper_path) = wallpapers.get(&key) else {
            continue;
        };
        let Some(monitor_device_path) = target_monitor_device_path(path) else {
            continue;
        };

        let _ = set_wallpaper_for_monitor(
            &session.desktop_wallpaper,
            &monitor_device_path,
            wallpaper_path,
        );
    }
}

fn capture_wallpaper_position() -> Option<DESKTOP_WALLPAPER_POSITION> {
    let session = create_desktop_wallpaper_session()?;
    unsafe { session.desktop_wallpaper.GetPosition().ok() }
}

fn best_effort_restore_wallpaper_position(position: Option<DESKTOP_WALLPAPER_POSITION>) {
    let Some(position) = position else {
        return;
    };
    let Some(session) = create_desktop_wallpaper_session() else {
        return;
    };
    let _ = unsafe { session.desktop_wallpaper.SetPosition(position) };
}

fn desired_output_index(desired: &Layout) -> HashMap<(u64, u32), &monarch::OutputConfig> {
    desired
        .outputs
        .iter()
        .map(|output| {
            (
                (output.display_id.adapter_luid, output.display_id.target_id),
                output,
            )
        })
        .collect()
}

fn path_target_key(path: &DISPLAYCONFIG_PATH_INFO) -> (u64, u32) {
    (
        luid_to_u64(
            path.targetInfo.adapterId.HighPart,
            path.targetInfo.adapterId.LowPart,
        ),
        path.targetInfo.id,
    )
}

fn apply_desired_source_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &mut [DISPLAYCONFIG_MODE_INFO],
    desired_output: Option<&&monarch::OutputConfig>,
) {
    let Some(output) = desired_output.copied() else {
        return;
    };
    if output.resolution.width == 0 || output.resolution.height == 0 {
        // Centinela de geometría (un monitor sembrado que nunca estuvo activo):
        // escribir 0x0 en el modo de origen haría fallar a `SetDisplayConfig` con
        // 87, o apilaría el monitor arriba del primario. Se deja el modo real del
        // snapshot como está y que lo ubique Windows.
        return;
    }

    // El índice viene de Windows, así que la guarda no es opcional: `get_mut`
    // devuelve `None` para el centinela 0xffff_ffff (path sin modo asignado) y
    // para cualquier índice corrupto, en vez de reventar.
    let mode_index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
    let Some(mode) = modes.get_mut(mode_index) else {
        return;
    };
    if mode.infoType.0 != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 {
        return;
    }

    // El chequeo de `infoType` de arriba es lo que hace legal leer esta rama de
    // la union: sin él, `sourceMode` estaría interpretando bytes de un modo de
    // target.
    unsafe {
        let source = &mut mode.Anonymous.sourceMode;
        source.position.x = output.position.x;
        source.position.y = output.position.y;
        source.width = output.resolution.width;
        source.height = output.resolution.height;
    }
}

fn apply_desired_target_refresh(
    path: &mut DISPLAYCONFIG_PATH_INFO,
    desired_output: Option<&&monarch::OutputConfig>,
) {
    let Some(output) = desired_output.copied() else {
        return;
    };
    // El `.max(1)` evita el denominador/numerador en cero, que Windows rechaza.
    let desired_refresh_mhz = output.refresh_rate_mhz.max(1);
    path.targetInfo.refreshRate.Numerator = desired_refresh_mhz;
    path.targetInfo.refreshRate.Denominator = 1000;
}

fn reorder_paths_for_desired_priority(
    paths: &mut [DISPLAYCONFIG_PATH_INFO],
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) {
    paths.sort_by(|left, right| {
        let left_rank = path_priority_rank(left, desired_outputs);
        let right_rank = path_priority_rank(right, desired_outputs);
        left_rank.cmp(&right_rank)
    });
}

/// El orden del array importa: Windows toma el primer path activo como el
/// primario. La tupla ordena primero por bucket (primario, activo, apagado,
/// desconocido) y después por posición, con luid+target de desempate para que el
/// orden sea determinista.
fn path_priority_rank(
    path: &DISPLAYCONFIG_PATH_INFO,
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) -> (u8, i32, i32, u64, u32) {
    let key = path_target_key(path);
    let Some(output) = desired_outputs.get(&key) else {
        return (3, 0, 0, key.0, key.1);
    };

    if !output.enabled {
        return (2, 0, 0, key.0, key.1);
    }

    let bucket = if output.primary { 0 } else { 1 };
    (bucket, output.position.y, output.position.x, key.0, key.1)
}

fn source_gdi_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    unsafe {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        };

        let status = DisplayConfigGetDeviceInfo(&mut source.header);
        if status != 0 {
            return None;
        }

        Some(wide_array_to_string(&source.viewGdiDeviceName))
    }
}

fn target_monitor_device_path(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    unsafe {
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };

        let status = DisplayConfigGetDeviceInfo(&mut target.header);
        if status != 0 {
            return None;
        }

        Some(wide_array_to_string(&target.monitorDevicePath))
    }
}

pub(super) fn target_advanced_color_enabled(path: &DISPLAYCONFIG_PATH_INFO) -> Option<bool> {
    unsafe {
        let mut info = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
        info.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            size: size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };

        let status = DisplayConfigGetDeviceInfo(&mut info.header);
        if status != 0 {
            return None;
        }

        // Bit 1 del bitfield = `advancedColorEnabled`. La union se lee por su
        // campo `value` justamente para no depender del layout de bitfields que
        // genere el binding.
        let flags = info.Anonymous.value;
        Some((flags & (1 << 1)) != 0)
    }
}

fn get_gamma_ramp_for_device(device_name: &str) -> Option<GammaRampWords> {
    let hdc = create_display_dc(device_name)?;
    let mut ramp = [0u16; GAMMA_RAMP_WORDS];
    let ok = unsafe { GetDeviceGammaRamp(hdc, ramp.as_mut_ptr().cast()) }.as_bool();
    unsafe {
        let _ = DeleteDC(hdc);
    }
    if ok {
        Some(ramp)
    } else {
        None
    }
}

fn set_gamma_ramp_for_device(device_name: &str, ramp: &GammaRampWords) -> bool {
    let Some(hdc) = create_display_dc(device_name) else {
        return false;
    };
    let ok = unsafe { SetDeviceGammaRamp(hdc, ramp.as_ptr().cast()) }.as_bool();
    unsafe {
        let _ = DeleteDC(hdc);
    }
    ok
}

fn create_display_dc(device_name: &str) -> Option<windows::Win32::Graphics::Gdi::HDC> {
    let device_wide = to_wide_null(device_name);
    let hdc = unsafe {
        CreateDCW(
            w!("DISPLAY"),
            PCWSTR(device_wide.as_ptr()),
            PCWSTR::null(),
            None,
        )
    };
    if hdc.is_invalid() {
        None
    } else {
        Some(hdc)
    }
}

fn to_wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Los arrays de Windows vienen rellenos con ceros, no terminados prolijamente:
/// se corta en el primer NUL, y si no hay ninguno se toma el array entero.
fn wide_array_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|ch| *ch == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

/// Sesión COM para hablar con `IDesktopWallpaper`.
///
/// El `Drop` es el que garantiza el `CoUninitialize`: sin él, cada captura o
/// restauración de wallpaper dejaría el apartment inicializado de más, y el
/// desbalance de COM se paga recién mucho después, en otro lado.
struct DesktopWallpaperSession {
    desktop_wallpaper: IDesktopWallpaper,
    should_uninitialize: bool,
}

impl Drop for DesktopWallpaperSession {
    fn drop(&mut self) {
        // Solo se desinicializa si esta sesión fue la que inicializó: si el hilo
        // ya venía con COM andando, desinicializarlo sería romperle el apartment
        // a otro.
        if self.should_uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn create_desktop_wallpaper_session() -> Option<DesktopWallpaperSession> {
    let mut should_uninitialize = false;
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() {
            should_uninitialize = true;
        }

        let desktop_wallpaper: IDesktopWallpaper =
            CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).ok()?;
        Some(DesktopWallpaperSession {
            desktop_wallpaper,
            should_uninitialize,
        })
    }
}

fn get_wallpaper_for_monitor(
    desktop_wallpaper: &IDesktopWallpaper,
    monitor_device_path: &str,
) -> Option<String> {
    let monitor_wide = to_wide_null(monitor_device_path);
    let wallpaper = unsafe {
        desktop_wallpaper
            .GetWallpaper(PCWSTR(monitor_wide.as_ptr()))
            .ok()?
    };

    // El string lo alocó COM: se copia a un `String` propio y recién ahí se
    // libera. Salir de acá sin el `CoTaskMemFree` es una fuga por cada monitor y
    // por cada apply.
    let wallpaper_path = unsafe { wallpaper.to_string().ok() };
    unsafe {
        CoTaskMemFree(Some(wallpaper.0.cast()));
    }
    wallpaper_path
}

fn set_wallpaper_for_monitor(
    desktop_wallpaper: &IDesktopWallpaper,
    monitor_device_path: &str,
    wallpaper_path: &str,
) -> bool {
    let monitor_wide = to_wide_null(monitor_device_path);
    let wallpaper_wide = to_wide_null(wallpaper_path);
    unsafe {
        desktop_wallpaper
            .SetWallpaper(
                PCWSTR(monitor_wide.as_ptr()),
                PCWSTR(wallpaper_wide.as_ptr()),
            )
            .is_ok()
    }
}
