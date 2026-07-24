// Audio por perfil — Displays v2, Fase 3.
//
// Cambia la salida de audio **por default de Windows** cuando se aplica un perfil
// (ej. el perfil "TV" manda el sonido a la TV). Reusa la misma maquinaria COM que
// el resto del módulo (mismo molde RAII que `DesktopWallpaperSession` en
// `apply.rs`): un apartment por llamada, inicializado en el hilo de turno y
// desinicializado en el `Drop`. Todo corre en hilos bloqueantes (spawn_blocking /
// watchdog / arranque), así que las llamadas COM van en línea, sin envolver.
//
// # Dos APIs, una documentada y otra no
//
// - **Enumerar / leer el default**: `IMMDeviceEnumerator` (MMDevice API,
//   documentada). Sirve para poblar el dropdown y para capturar el default previo
//   (para el rollback).
// - **Setear el default**: `IPolicyConfig`, interfaz COM **no documentada** —
//   Microsoft no publica API para escribir el default, solo para leer. Es la que
//   usa el propio panel de Sonido + EarTrumpet/NirCmd/SoundVolumeView desde hace
//   ~15 años. Se declara **a mano** (abajo) para no arrastrar un segundo `windows`
//   0.61 vía el crate `com-policy-config` (el proyecto pinnea 0.60). Si algún
//   update de Windows la rompe, el video no se afecta: es best-effort y se loguea.
//
// # panic="abort"
//
// Un panic acá se lleva puesto TODO el proceso (clipboard + discovery). Por eso
// NADA de `unwrap`/`expect`/indexing crudo: todo es `Result`/`Option`, y ante
// error se loguea y se sigue. El video NUNCA se revierte porque el audio falló.
#![cfg(target_os = "windows")]

use core::ffi::c_void;

use monarch::AudioTarget;
use windows::core::{Interface, GUID, HRESULT, IUnknown, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, ERole, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED, STGM_READ,
};

// ── IPolicyConfig (declarado a mano) ─────────────────────────────────────────
//
// GUIDs verificados contra IPolicyConfig.h (usado por EarTrumpet/OpenVR-Advanced
// Settings/com-policy-config). El vtable respeta el orden EXACTO de la interfaz:
// `SetDefaultEndpoint` es el método #11 (tras los 3 de IUnknown). Los métodos
// que no usamos se declaran como slots opacos (`usize`) para mantener el layout
// del vtable sin tener que tipar sus firmas.

/// CLSID del `PolicyConfigClient` (el objeto COM que expone `IPolicyConfig`).
const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[repr(transparent)]
#[derive(Clone)]
struct IPolicyConfig(IUnknown);

unsafe impl Interface for IPolicyConfig {
    type Vtable = IPolicyConfigVtbl;
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

#[repr(C)]
#[allow(non_snake_case)]
struct IPolicyConfigVtbl {
    base__: windows::core::IUnknown_Vtbl,
    // Métodos 1-10 (formatos, períodos, share mode, property values): no los
    // usamos, pero ocupan su slot en el vtable.
    GetMixFormat: usize,
    GetDeviceFormat: usize,
    ResetDeviceFormat: usize,
    SetDeviceFormat: usize,
    GetProcessingPeriod: usize,
    SetProcessingPeriod: usize,
    GetShareMode: usize,
    SetShareMode: usize,
    GetPropertyValue: usize,
    SetPropertyValue: usize,
    // Método 11 — el único que llamamos.
    SetDefaultEndpoint:
        unsafe extern "system" fn(this: *mut c_void, wszDeviceId: PCWSTR, eRole: ERole) -> HRESULT,
    SetEndpointVisibility: usize,
}

impl IPolicyConfig {
    /// Setea `device_id` como default para `role`. `HRESULT` crudo (best-effort).
    unsafe fn set_default_endpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT {
        (Interface::vtable(self).SetDefaultEndpoint)(Interface::as_raw(self), device_id, role)
    }
}

/// Variante Vista de la interfaz: mismo objeto COM (PolicyConfigClient) y mismo
/// layout de vtable, otro IID. Fallback para algunos Win10 viejos donde el
/// `QueryInterface` del IID Win7+ rebota.
#[repr(transparent)]
#[derive(Clone)]
struct IPolicyConfigVista(IUnknown);

unsafe impl Interface for IPolicyConfigVista {
    type Vtable = IPolicyConfigVtbl;
    const IID: GUID = GUID::from_u128(0x568b9108_44bf_40b4_9006_86afe5b5a620);
}

/// Instancia `IPolicyConfig` con la cascada de IIDs del spec: primero el IID
/// Win7+ (el que anda en Win11, el target de Guido) y, si rebota, el IID Vista
/// (mismo vtable ⇒ se re-envuelve). NO se implementa la tercera variante
/// (`IPolicyConfig10`, que usa el IID de `IUnknown`): es rara y el target es
/// Win11. Best-effort: si ninguna instancia, el audio no cambia y se loguea.
unsafe fn crear_policy_config() -> Option<IPolicyConfig> {
    if let Ok(config) =
        CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
    {
        return Some(config);
    }
    if let Ok(vista) =
        CoCreateInstance::<_, IPolicyConfigVista>(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
    {
        // Mismo objeto COM y mismo layout de vtable: re-envolver el IUnknown.
        return Some(IPolicyConfig(vista.0));
    }
    None
}

/// Los tres roles que hay que setear para que "todo el sonido" vaya a la salida:
/// con uno solo no alcanza (música/llamadas pueden quedar en la salida vieja).
const ROLES: [ERole; 3] = [eConsole, eMultimedia, eCommunications];

// ── Apartment COM RAII (mismo criterio que DesktopWallpaperSession) ───────────

struct ComApartment {
    should_uninitialize: bool,
}

impl ComApartment {
    fn enter() -> Self {
        let mut should_uninitialize = false;
        unsafe {
            // S_OK o S_FALSE ⇒ is_ok() ⇒ esta llamada inicializó (o sumó un
            // refcount) y debe pagarse con un CoUninitialize. RPC_E_CHANGED_MODE
            // (el hilo ya tenía COM en otro modo) ⇒ is_err() ⇒ no tocar el balance.
            if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() {
                should_uninitialize = true;
            }
        }
        Self {
            should_uninitialize,
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

// ── Estado del rollback de audio ─────────────────────────────────────────────

/// El default de audio que había **antes** de aplicar un perfil, por los 3 roles.
/// Se captura al aplicar (solo si el perfil cambia el audio) y se consume en el
/// revert (manual o por timeout). `None` en un rol = ese rol no tenía default
/// legible (raro; se saltea al restaurar).
#[derive(Clone, Debug)]
pub struct AudioPrevio {
    console: Option<AudioTarget>,
    multimedia: Option<AudioTarget>,
    communications: Option<AudioTarget>,
}

impl AudioPrevio {
    /// `true` si algún rol tenía un default capturado (algo que restaurar).
    pub fn tiene_algo(&self) -> bool {
        self.console.is_some() || self.multimedia.is_some() || self.communications.is_some()
    }
}

// ── API pública del módulo ───────────────────────────────────────────────────

/// Enumera las salidas de audio **activas** (id opaco + nombre lindo). Puebla el
/// dropdown "Sonido a:" del frontend. Vacío ante cualquier fallo COM.
pub fn listar_salidas() -> Vec<AudioTarget> {
    let _com = ComApartment::enter();
    unsafe {
        match CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(enumerator) => enumerar_activas(&enumerator),
            Err(e) => {
                crate::runtime_log::warn(format!(
                    "[displays] audio: no se pudieron enumerar las salidas: {e}"
                ));
                Vec::new()
            }
        }
    }
}

/// Captura el default de audio ACTUAL por los 3 roles, para poder restaurarlo si
/// el cambio de monitores se revierte. `None` si no se pudo abrir COM.
pub fn capturar_default() -> Option<AudioPrevio> {
    let _com = ComApartment::enter();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        Some(AudioPrevio {
            console: leer_default(&enumerator, eConsole),
            multimedia: leer_default(&enumerator, eMultimedia),
            communications: leer_default(&enumerator, eCommunications),
        })
    }
}

/// Setea `target` como default en los 3 roles. `Ok(true)` si lo cambió, `Ok(false)`
/// si la salida NO está presente (TV apagada) — el llamador avisa y NO revierte el
/// video. `Err` solo si COM falla al crear los objetos.
pub fn aplicar_salida(target: &AudioTarget) -> Result<bool, String> {
    let _com = ComApartment::enter();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("no se pudo crear el enumerador de audio: {e}"))?;
        let activas = enumerar_activas(&enumerator);
        let Some(id) = resolver(&activas, target) else {
            // Salida no presente: ni id ni friendly_name matchean una activa.
            return Ok(false);
        };
        setear_los_tres_roles(&id)?;
        Ok(true)
    }
}

/// Restaura el default previo capturado (best-effort, nunca falla hacia afuera).
/// Se llama en el revert manual y en el auto-revert por timeout.
pub fn restaurar_default(previo: &AudioPrevio) {
    let _com = ComApartment::enter();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(e) => {
                    crate::runtime_log::warn(format!(
                        "[displays] audio: no se pudo restaurar (enumerador): {e}"
                    ));
                    return;
                }
            };
        let config = match crear_policy_config() {
            Some(c) => c,
            None => {
                crate::runtime_log::warn(
                    "[displays] audio: no se pudo restaurar (IPolicyConfig no instanció)",
                );
                return;
            }
        };
        let activas = enumerar_activas(&enumerator);
        restaurar_rol(&config, &activas, eConsole, previo.console.as_ref());
        restaurar_rol(&config, &activas, eMultimedia, previo.multimedia.as_ref());
        restaurar_rol(
            &config,
            &activas,
            eCommunications,
            previo.communications.as_ref(),
        );
    }
}

// ── Helpers internos ─────────────────────────────────────────────────────────

/// Todas las salidas de render activas como `AudioTarget` (id + friendly).
unsafe fn enumerar_activas(enumerator: &IMMDeviceEnumerator) -> Vec<AudioTarget> {
    let mut out = Vec::new();
    let collection = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
        Ok(c) => c,
        Err(_) => return out,
    };
    let count = collection.GetCount().unwrap_or(0);
    for i in 0..count {
        let Ok(device) = collection.Item(i) else {
            continue;
        };
        let Some(endpoint_id) = tomar_id(&device) else {
            continue;
        };
        let friendly_name = tomar_friendly(&device).unwrap_or_default();
        out.push(AudioTarget {
            endpoint_id,
            friendly_name,
        });
    }
    out
}

/// El default actual de `role` como `AudioTarget`.
unsafe fn leer_default(enumerator: &IMMDeviceEnumerator, role: ERole) -> Option<AudioTarget> {
    let device = enumerator.GetDefaultAudioEndpoint(eRender, role).ok()?;
    let endpoint_id = tomar_id(&device)?;
    let friendly_name = tomar_friendly(&device).unwrap_or_default();
    Some(AudioTarget {
        endpoint_id,
        friendly_name,
    })
}

/// Resuelve `target` contra las salidas activas: primero por `endpoint_id`, y si
/// no resuelve (driver reinstalado, USB a otro puerto) por `friendly_name`. `None`
/// = la salida no está presente ⇒ no re-ruteamos a ciegas.
fn resolver(activas: &[AudioTarget], target: &AudioTarget) -> Option<String> {
    if activas.iter().any(|d| d.endpoint_id == target.endpoint_id) {
        return Some(target.endpoint_id.clone());
    }
    activas
        .iter()
        .find(|d| !target.friendly_name.is_empty() && d.friendly_name == target.friendly_name)
        .map(|d| d.endpoint_id.clone())
}

/// Setea `endpoint_id` como default en los 3 roles vía IPolicyConfig. Best-effort:
/// intenta LOS TRES aunque uno falle (no corta en el primero), para no dejar un
/// cambio a medias evitable; devuelve `Err` si alguno falló, listando cuáles.
unsafe fn setear_los_tres_roles(endpoint_id: &str) -> Result<(), String> {
    let config = crear_policy_config()
        .ok_or_else(|| "no se pudo crear IPolicyConfig (ni IID Win7+ ni Vista)".to_string())?;
    let wide = a_wide(endpoint_id);
    let mut fallos = Vec::new();
    for role in ROLES {
        let hr = config.set_default_endpoint(PCWSTR(wide.as_ptr()), role);
        if hr.is_err() {
            fallos.push(format!("{role:?}={hr:?}"));
        }
    }
    if fallos.is_empty() {
        Ok(())
    } else {
        Err(format!("SetDefaultEndpoint falló en {}", fallos.join(", ")))
    }
}

/// Restaura un rol a su default previo, resolviéndolo entre las activas actuales
/// (fallback por friendly). Best-effort: si el device previo ya no está, no toca.
unsafe fn restaurar_rol(
    config: &IPolicyConfig,
    activas: &[AudioTarget],
    role: ERole,
    previo: Option<&AudioTarget>,
) {
    let Some(previo) = previo else {
        return;
    };
    let Some(id) = resolver(activas, previo) else {
        return;
    };
    let wide = a_wide(&id);
    let _ = config.set_default_endpoint(PCWSTR(wide.as_ptr()), role);
}

/// `IMMDevice::GetId` → `String` propio, liberando el buffer COM.
unsafe fn tomar_id(device: &IMMDevice) -> Option<String> {
    let raw = device.GetId().ok()?;
    pwstr_a_string_liberando(raw)
}

/// `PKEY_Device_FriendlyName` del property store → `String` propio.
unsafe fn tomar_friendly(device: &IMMDevice) -> Option<String> {
    let store = device.OpenPropertyStore(STGM_READ).ok()?;
    let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    // PropVariantToStringAlloc aloca con CoTaskMemAlloc; el PROPVARIANT se limpia
    // solo en su Drop (PropVariantClear).
    let raw = PropVariantToStringAlloc(&value).ok()?;
    pwstr_a_string_liberando(raw)
}

/// Copia un `PWSTR` alocado por COM a un `String` propio y libera el buffer.
unsafe fn pwstr_a_string_liberando(raw: PWSTR) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    let s = raw.to_string().ok();
    CoTaskMemFree(Some(raw.0 as *const c_void));
    s
}

/// UTF-16 NUL-terminado, para pasar como `PCWSTR`. El `Vec` debe seguir vivo
/// mientras el `PCWSTR` se use.
fn a_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
