// Motor de acciones — Displays v2, Fase 4 (perfiles como escenas).
//
// Cuando un perfil-escena queda COMMITTEADO (ver el ciclo de escena en `mod.rs`),
// se corren sus acciones de ENTRADA; al pasar a otro perfil, las de SALIDA del
// anterior. Este módulo solo sabe EJECUTAR una lista de acciones; el "cuándo" y
// el "de quién" lo decide el enganche del ciclo.
//
// # Dos vías de lanzamiento, ninguna por shell
//
// - **`.exe` + args**: `std::process::Command` con los args EN LISTA (nunca un
//   string de shell ni `cmd /c`) y `CREATE_NO_WINDOW` para no abrir una consola.
//   Un nombre pelado (`chrome.exe`) que no esté en el PATH se resuelve por el
//   registro (App Paths). Sin concatenación ⇒ sin inyección (Riesgos del spec).
// - **URI de protocolo (`steam://…`) o `.lnk`**: van por `ShellExecuteW` con el
//   verbo `"open"`, que delega en el handler del SO (y arranca la app si está
//   cerrada). `Command` NO puede lanzar ni una URI ni un `.lnk` (CreateProcess
//   solo corre `.exe`), de ahí el dispatch por tipo de destino.
//
// Se usa el crate `windows` (no `tauri-plugin-opener`) a propósito: este módulo,
// como el resto del motor de displays, **no menciona a Tauri**, así se type-checkea
// en un crate scratch con solo `windows` (ver la cabecera de `mod.rs` y
// `docs/DECISIONS.md`). Es el mismo criterio que `audio.rs`.
//
// # Volumen (COM)
//
// `IAudioEndpointVolume` sobre la salida por default actual. Mismo molde RAII de
// apartment COM que `audio.rs` (Fase 3).
//
// # panic="abort" ⇒ best-effort a rajatabla
//
// Un panic acá se lleva puesto TODO el proceso (clipboard + discovery). NADA de
// `unwrap`/`expect`: cada acción que falla se loguea y se SIGUE con la próxima.
// El cambio de monitores/audio ya aplicado NUNCA se revierte porque una acción
// falló (Supuestos [ALTO] del spec).
#![cfg(target_os = "windows")]

use std::os::windows::process::CommandExt;
use std::process::Command;

use monarch::Accion;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Flag de `CreateProcess` para no abrir ventana de consola al lanzar un `.exe`.
/// Constante estable de Win32 (`winbase.h`); se hardcodea para no tipar el
/// `PROCESS_CREATION_FLAGS` del crate `windows` solo para esto.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Corre una lista de acciones EN ORDEN, best-effort. Pensada para llamarse desde
/// un contexto bloqueante (`spawn_blocking` / watchdog / arranque), igual que
/// `audio::aplicar_salida`: las llamadas COM y el `spawn` van en línea.
pub fn ejecutar(acciones: &[Accion]) {
    for accion in acciones {
        match accion {
            Accion::Lanzar { destino, args } => lanzar(destino, args),
            Accion::Volumen { nivel } => fijar_volumen(*nivel),
        }
    }
}

// ── Lanzar ───────────────────────────────────────────────────────────────────

fn lanzar(destino: &str, args: &[String]) {
    let destino = destino.trim();
    if destino.is_empty() {
        crate::runtime_log::warn("[displays] acción Lanzar con destino vacío; se saltea");
        return;
    }

    // URI de protocolo (steam://…) o acceso directo .lnk: los abre el handler del
    // SO. `Command` no puede con ninguno de los dos. Los args no aplican acá.
    if destino.contains("://") || termina_en(destino, ".lnk") {
        shell_open(destino);
        return;
    }

    // Ejecutable: args EN LISTA (sin shell), ventana oculta.
    let programa = resolver_ejecutable(destino);
    let mut cmd = Command::new(&programa);
    cmd.args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    match cmd.spawn() {
        Ok(_) => crate::runtime_log::info(format!(
            "[displays] acción: lanzado «{programa}» ({} arg/s)",
            args.len()
        )),
        Err(e) => crate::runtime_log::warn(format!(
            "[displays] no se pudo lanzar «{destino}»: {e}"
        )),
    }
}

/// Abre una URI de protocolo o un `.lnk` con el verbo `"open"` (ShellExecuteW).
/// El retorno `HINSTANCE` > 32 = ok; ≤ 32 = error (contrato de ShellExecute).
fn shell_open(destino: &str) {
    let file = a_wide(destino);
    let verb = a_wide("open");
    unsafe {
        let hinst = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if (hinst.0 as isize) <= 32 {
            crate::runtime_log::warn(format!(
                "[displays] no se pudo abrir «{destino}» (ShellExecute={})",
                hinst.0 as isize
            ));
        } else {
            crate::runtime_log::info(format!("[displays] acción: abierto «{destino}»"));
        }
    }
}

/// `true` si `s` termina en `suf` sin distinguir mayúsculas (ASCII).
fn termina_en(s: &str, suf: &str) -> bool {
    let s = s.as_bytes();
    let suf = suf.as_bytes();
    s.len() >= suf.len() && s[s.len() - suf.len()..].eq_ignore_ascii_case(suf)
}

/// Resuelve el ejecutable a lanzar. Si `destino` trae separador de ruta es una
/// ruta explícita y se usa tal cual; si es un nombre pelado (`chrome.exe`) se
/// intenta resolver por App Paths del registro (así el preset de Chrome anda sin
/// pedir la ruta completa). Si no resuelve, se devuelve el nombre y `Command`
/// lo buscará en el PATH.
fn resolver_ejecutable(destino: &str) -> String {
    if destino.contains('\\') || destino.contains('/') {
        return destino.to_string();
    }
    buscar_app_paths(destino).unwrap_or_else(|| destino.to_string())
}

/// Ruta completa de un `.exe` registrado en App Paths (Chrome, Steam, etc.),
/// buscando primero en HKCU y después en HKLM. `None` si no está registrado.
fn buscar_app_paths(exe: &str) -> Option<String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let sub = format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}");
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let root = RegKey::predef(hive);
        let Ok(key) = root.open_subkey(&sub) else {
            continue;
        };
        // El valor por default (nombre "") de la clave es la ruta completa.
        if let Ok(ruta) = key.get_value::<String, _>("") {
            let ruta = ruta.trim().trim_matches('"').trim().to_string();
            if !ruta.is_empty() {
                return Some(ruta);
            }
        }
    }
    None
}

// ── Volumen (COM) ─────────────────────────────────────────────────────────────

/// Apartment COM RAII (mismo criterio que `audio::ComApartment` / la sesión de
/// wallpaper en `apply.rs`): inicializa COM en el hilo de turno y lo desinicializa
/// en el `Drop`. Copiado a propósito (reuso por copia en v1, spec).
struct ComApartment {
    should_uninitialize: bool,
}

impl ComApartment {
    fn enter() -> Self {
        let mut should_uninitialize = false;
        unsafe {
            // S_OK o S_FALSE ⇒ is_ok() ⇒ hay que pagar el CoUninitialize.
            // RPC_E_CHANGED_MODE ⇒ is_err() ⇒ no tocar el balance.
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

/// Fija el volumen (0–100) de la salida de audio por default actual. Best-effort:
/// si no hay salida o COM falla, se loguea y sigue. Corre DESPUÉS del ruteo de
/// audio de Fase 3, así que la salida por default ya es la del perfil.
fn fijar_volumen(nivel: u8) {
    let nivel = nivel.min(100);
    let scalar = nivel as f32 / 100.0;
    let _com = ComApartment::enter();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(e) => {
                    crate::runtime_log::warn(format!(
                        "[displays] volumen: no se pudo crear el enumerador de audio: {e}"
                    ));
                    return;
                }
            };
        let device = match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
            Ok(d) => d,
            Err(e) => {
                crate::runtime_log::warn(format!(
                    "[displays] volumen: no hay salida de audio por default: {e}"
                ));
                return;
            }
        };
        let volume: IAudioEndpointVolume = match device.Activate(CLSCTX_ALL, None) {
            Ok(v) => v,
            Err(e) => {
                crate::runtime_log::warn(format!(
                    "[displays] volumen: no se pudo activar el control de volumen: {e}"
                ));
                return;
            }
        };
        // El contexto de evento va en null: no escuchamos callbacks de cambio.
        if let Err(e) = volume.SetMasterVolumeLevelScalar(scalar, std::ptr::null()) {
            crate::runtime_log::warn(format!(
                "[displays] volumen: no se pudo fijar el nivel {nivel}%: {e}"
            ));
        } else {
            crate::runtime_log::info(format!("[displays] acción: volumen fijado en {nivel}%"));
        }
    }
}

/// UTF-16 NUL-terminado para pasar como `PCWSTR`. El `Vec` debe seguir vivo
/// mientras el `PCWSTR` se use.
fn a_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
