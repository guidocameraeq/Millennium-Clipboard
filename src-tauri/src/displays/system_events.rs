// El listener de eventos del sistema — SPEC-displays, Fase 2 (resume) + Fase 3
// (cambio de topología).
//
// La ventana oculta reacciona a DOS mensajes de Windows, por CANALES SEPARADOS:
//  - `WM_POWERBROADCAST` (resume): segunda pieza de la red de seguridad (la
//    primera es `watchdog.rs`). Al despertar, tira el cache.
//  - `WM_DISPLAYCHANGE` (Fase 3): enchufaste/desenchufaste un monitor, o un apply
//    propio cambió la topología. **Refresca la vista sin tocar el cache.**
//
// # Qué problema resuelve
//
// Cuando la máquina se suspende y despierta, Windows puede **renumerar las
// placas de video**: los `adapter_luid` cambian. Todo lo que el motor tenga
// cacheado de antes de dormir queda apuntando a identificadores que ya no
// existen, y además la enumeración justo al despertar suele fallar al leer los
// EDID (los paneles todavía están arrancando), así que un cache armado en ese
// momento queda envenenado con datos basura.
//
// El efecto para el usuario es que después de suspender, la TV "no está" aunque
// esté enchufada, o peor: un attach apunta al monitor equivocado.
//
// La cura es simple y es la del donante: al despertar, **tirar el cache** y que
// la próxima consulta reconstruya todo desde una enumeración fresca.
//
// # WM_DISPLAYCHANGE refresca, pero NO invalida el cache (la cicatriz)
//
// La tentación al recibir `WM_DISPLAYCHANGE` es invalidar el cache, como hace el
// resume. **Es un error**, y el donante lo dejó anotado: el cache es justamente
// lo que mantiene vivo el recuerdo de un monitor *detachado*, y un apply propio
// dispara `WM_DISPLAYCHANGE` — invalidar ahí borraría el monitor que acabamos de
// apagar y con él la posibilidad de volver a prenderlo.
//
// Por eso el cambio de topología va por un canal PROPIO (`canal_cambio`), con su
// propio callback, que solo refresca la vista (el frontend re-consulta y
// re-enumera fresco). El cache se tira **solo** en el resume (`canal`). Los dos
// caminos no se cruzan.
//
// # Costo en reposo
//
// Cero. `GetMessageW` bloquea el hilo hasta que llega un mensaje; no hay polling.
// Es la razón de que sea una ventana y no un timer.
#![cfg(target_os = "windows")]

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, WM_DISPLAYCHANGE,
    WM_POWERBROADCAST, WNDCLASSW,
};

use super::diagnostics;

/// Al despertar llegan varias notificaciones seguidas y el stack de video tarda
/// en asentarse. Se absorbe la ráfaga y se actúa una sola vez, al final.
const ESPERA_DE_ASENTADO: Duration = Duration::from_millis(2000);

/// Un cambio de topología también llega en ráfaga (una TV negociando modo dispara
/// varios `WM_DISPLAYCHANGE`). Se coalescen, pero con una espera mucho más corta
/// que el resume: acá se quiere que la lista reaccione rápido al enchufe.
const ESPERA_DE_CAMBIO: Duration = Duration::from_millis(400);

/// Canal del resume. Es un `OnceLock` para que `spawn` sea idempotente: registrar
/// dos veces la misma clase de ventana falla, y dos listeners duplicarían el
/// trabajo.
fn canal() -> &'static OnceLock<Mutex<Sender<()>>> {
    static EMISOR: OnceLock<Mutex<Sender<()>>> = OnceLock::new();
    &EMISOR
}

/// Canal del cambio de topología (`WM_DISPLAYCHANGE`). Separado del resume a
/// propósito: este NO invalida el cache (ver cabecera).
fn canal_cambio() -> &'static OnceLock<Mutex<Sender<()>>> {
    static EMISOR: OnceLock<Mutex<Sender<()>>> = OnceLock::new();
    &EMISOR
}

/// Arranca el listener. `al_despertar` y `al_cambiar` se ejecutan en hilos
/// propios, una vez por ráfaga, **nunca** en el hilo de la ventana (bloquear el
/// bombeo de mensajes de Windows es una forma conocida de colgar el escritorio).
pub(super) fn spawn(
    al_despertar: Box<dyn Fn() + Send + 'static>,
    al_cambiar: Box<dyn Fn() + Send + 'static>,
) {
    let (emisor_resume, receptor_resume) = mpsc::channel::<()>();
    if canal().set(Mutex::new(emisor_resume)).is_err() {
        diagnostics::log("system_events:ya_estaba_arrancado");
        return;
    }
    let (emisor_cambio, receptor_cambio) = mpsc::channel::<()>();
    if canal_cambio().set(Mutex::new(emisor_cambio)).is_err() {
        // El canal del resume ya se seteó bien arriba, así que llegar acá con este
        // ocupado no debería pasar; se anota y no se arranca el consumidor.
        diagnostics::log("system_events:canal_cambio_ya_estaba");
        return;
    }

    std::thread::spawn(bombear_mensajes);
    std::thread::spawn(move || consumir(receptor_resume, al_despertar));
    std::thread::spawn(move || consumir_cambio(receptor_cambio, al_cambiar));
}

/// Llamado desde el wndproc ante un resume. No toma ningún lock del resto de la
/// app: solo empuja al canal.
fn notificar() {
    let Some(emisor) = canal().get() else {
        return;
    };
    let Ok(emisor) = emisor.lock() else {
        return;
    };
    let _ = emisor.send(());
}

/// Llamado desde el wndproc ante un `WM_DISPLAYCHANGE`. Igual que `notificar`
/// pero por el canal del cambio de topología.
fn notificar_cambio() {
    let Some(emisor) = canal_cambio().get() else {
        return;
    };
    let Ok(emisor) = emisor.lock() else {
        return;
    };
    let _ = emisor.send(());
}

fn consumir(receptor: mpsc::Receiver<()>, al_despertar: Box<dyn Fn() + Send + 'static>) {
    loop {
        // Bloquea hasta el primer aviso de la ráfaga.
        if receptor.recv().is_err() {
            // El emisor murió: la app se está cerrando.
            return;
        }

        // Absorbe el resto de la ráfaga. Cada aviso nuevo reinicia la espera.
        loop {
            match receptor.recv_timeout(ESPERA_DE_ASENTADO) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        diagnostics::log("system_events:resume:invalidando_cache");
        al_despertar();
    }
}

/// Igual que `consumir` pero para el cambio de topología: absorbe la ráfaga con
/// una espera más corta y refresca la vista. **No invalida el cache** — eso lo
/// decide el callback (`al_cambiar`), que solo avisa el cambio.
fn consumir_cambio(receptor: mpsc::Receiver<()>, al_cambiar: Box<dyn Fn() + Send + 'static>) {
    loop {
        if receptor.recv().is_err() {
            return;
        }

        loop {
            match receptor.recv_timeout(ESPERA_DE_CAMBIO) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        diagnostics::log("system_events:displaychange:refrescando_vista");
        al_cambiar();
    }
}

/// La ventana oculta y su bombeo de mensajes.
///
/// **A propósito NO es una ventana message-only** (`HWND_MESSAGE`): esas nunca
/// reciben mensajes de broadcast como `WM_POWERBROADCAST`. Tiene que ser una
/// top-level, aunque sea de 0×0 y sin `WS_VISIBLE`. Es una piedra con la que el
/// donante ya tropezó.
fn bombear_mensajes() {
    unsafe {
        let instancia = match GetModuleHandleW(None) {
            Ok(instancia) => instancia,
            Err(err) => {
                diagnostics::log(format!("system_events:error:get_module_handle:{err}"));
                return;
            }
        };

        // Nombre propio de Millennium: si algún día conviven Monarch y
        // Millennium en la misma sesión, no se pisan la clase de ventana.
        let clase = w!("MillenniumDisplaysSystemEvents");
        let definicion = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instancia.into(),
            lpszClassName: clase,
            ..Default::default()
        };
        if RegisterClassW(&definicion) == 0 {
            diagnostics::log("system_events:error:register_class_failed");
            return;
        }

        let ventana = match CreateWindowExW(
            Default::default(),
            clase,
            w!("Millennium Displays System Events"),
            Default::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instancia.into()),
            None,
        ) {
            Ok(ventana) => ventana,
            Err(err) => {
                diagnostics::log(format!("system_events:error:create_window:{err}"));
                return;
            }
        };
        let _ = ventana;

        diagnostics::log("system_events:listener_arrancado");
        let mut mensaje = MSG::default();
        // `GetMessageW` devuelve >0 mientras haya mensajes, 0 en WM_QUIT y -1 ante
        // error. Salir del `while` por cualquiera de las dos últimas significa que
        // la protección de resume se apagó: queda anotado, no se pierde en silencio.
        while GetMessageW(&mut mensaje, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&mensaje);
            DispatchMessageW(&mensaje);
        }
        diagnostics::log("system_events:error:el_bombeo_de_mensajes_termino");
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_POWERBROADCAST {
        let evento = wparam.0 as u32;
        if evento == PBT_APMRESUMEAUTOMATIC || evento == PBT_APMRESUMESUSPEND {
            notificar();
        }
        // Windows espera TRUE para los mensajes de energía que uno maneja.
        return LRESULT(1);
    }
    if msg == WM_DISPLAYCHANGE {
        // Fase 3: refrescar la vista SIN invalidar el cache (canal aparte del
        // resume, ver cabecera). Es solo una notificación; se avisa y se cae al
        // default igual (no se consume el mensaje).
        notificar_cambio();
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
