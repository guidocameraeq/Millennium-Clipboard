// El watchdog de auto-rollback — SPEC-displays, Fase 2.
//
// # Por qué existe este archivo
//
// La red de seguridad que impide que un cambio de monitores te deje la máquina
// inusable necesita **DOS piezas**, y solo una viene del crate `monarch`:
//
//   1. **El manager (política, ya existe)**: al aplicar un layout guarda el
//      anterior y un plazo. Expone `rollback_if_confirmation_expired()`, que
//      revierte *si* el plazo venció. Es **PASIVO**: nunca se despierta solo.
//   2. **El gatillo (esto)**: alguien tiene que preguntarle, al vencer el plazo,
//      si hay que revertir. Sin esta pieza el manager guarda un plazo que nadie
//      consulta, el layout malo queda pegado para siempre y no hay forma de
//      volver — que es exactamente el bug que Monarch nació para matar.
//
// Está documentado como riesgo de máxima consecuencia en `docs/SPEC-displays.md`
// y en la "Doctrina CCD heredada" de `docs/DECISIONS.md`. **Prohibido
// simplificarlo.**
//
// # Por qué NO es una copia literal del donante
//
// El watchdog de Monarch (`app/events.rs`) duerme exactamente el plazo y recién
// entonces pregunta "¿venció?". `Instant::elapsed() >= timeout` y
// `thread::sleep(timeout)` usan el mismo reloj monotónico, así que en la
// práctica da verdadero — pero es una carrera sin red: si por lo que fuera
// despierta un microsegundo antes, la respuesta es "todavía no", el hilo se
// muere ahí y **nadie vuelve a preguntar nunca**. El costo de esa carrera es la
// máquina inusable; el costo de cubrirla son diez líneas.
//
// Acá se cubre con dos cosas: un **margen** sobre el plazo, y un **lazo de
// reintento acotado** que vuelve a dormir lo que falte si al despertar todavía
// hay algo pendiente. El lazo sirve además para el otro caso feo: que el
// rollback *falle* (Windows rechaza el cambio). El manager, cuando eso pasa,
// **conserva** la confirmación pendiente en vez de tirarla, así que reintentar
// tiene sentido.
//
// # Forma del archivo
//
// La decisión ("¿revierto, espero, me voy?") está separada del efecto (dormir,
// avisarle al frontend). Por eso este archivo **no menciona a Tauri ni al crate
// `windows`**: así se puede compilar y **testear de verdad** en una máquina que
// no puede linkear contra `windows` (falta `dlltool.exe`; ver `docs/DECISIONS.md`).
// El hilo y los eventos viven en la glue, en `mod.rs`.
#![cfg(target_os = "windows")]

use std::sync::Mutex;
use std::time::Duration;

use monarch::{ConfigStore, DisplayBackend, MonarchDisplayManager};

use super::diagnostics;

/// Cuánto se duerme de más sobre el plazo antes de preguntar si venció.
///
/// Cubre la carrera descrita arriba. No se le nota al usuario: el countdown de
/// la UI ya llegó a cero y está esperando.
const MARGEN: Duration = Duration::from_millis(300);

/// Piso de espera entre vueltas. Sin esto, un `remaining` de cero que todavía no
/// cuenta como vencido haría girar el lazo en vacío quemando CPU — y el consumo
/// en reposo es algo que este proyecto vigila.
const ESPERA_MINIMA: Duration = Duration::from_millis(250);

/// Cuánto se espera antes de reintentar un rollback que falló.
const ESPERA_TRAS_FALLO: Duration = Duration::from_millis(750);

/// Tope de vueltas. Es la garantía de que el hilo termina siempre.
///
/// Con el plazo por defecto (10 s) la primera vuelta ya resuelve el 100% de los
/// casos normales; las otras cuatro son para reintentar un rollback rechazado.
const MAX_VUELTAS: u32 = 5;

/// Lo que el watchdog decidió en una vuelta.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    /// Venció y se revirtió. Terminó el trabajo.
    Revertido,
    /// Ya no hay nada pendiente: el usuario confirmó, o revirtió a mano, o otro
    /// watchdog llegó primero. Nada que hacer.
    NadaPendiente,
    /// Sigue pendiente pero **todavía no venció**. Hay que dormir esto y volver.
    Esperar(Duration),
    /// El rollback se intentó y Windows lo rechazó. El manager conservó la
    /// confirmación pendiente, así que se puede reintentar.
    Fallo(String),
}

/// El desenlace del watchdog completo, que es lo que se le informa al usuario.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Desenlace {
    /// Revirtió solo por vencimiento del plazo.
    Revertido,
    /// No hizo falta: alguien resolvió la confirmación antes.
    NadaQueHacer,
    /// **Caso grave**: se agotaron los reintentos y el layout sigue aplicado sin
    /// confirmar. El usuario tiene que enterarse SÍ o SÍ.
    NoPudoRevertir(String),
}

/// Una vuelta de decisión. Sin efectos: no duerme, no avisa, no loguea.
///
/// Se consulta `has_pending_confirmation()` **antes** de pedir el rollback para
/// poder distinguir "no venció todavía" de "no hay nada": las dos cosas hacen
/// que `rollback_if_confirmation_expired()` devuelva `Ok(false)`, y confundirlas
/// es la diferencia entre esperar otra vuelta o abandonar el puesto.
pub(super) fn decidir<B, S>(manager: &mut MonarchDisplayManager<B, S>) -> Decision
where
    B: DisplayBackend,
    S: ConfigStore,
{
    if !manager.has_pending_confirmation() {
        return Decision::NadaPendiente;
    }

    match manager.rollback_if_confirmation_expired() {
        Ok(true) => Decision::Revertido,
        Ok(false) => match manager.pending_confirmation_remaining() {
            // Todavía le queda plazo. Se duerme lo que falte, más el margen.
            Some(restante) => Decision::Esperar(restante.saturating_add(MARGEN)),
            // Se resolvió entre la consulta de arriba y ésta (otro hilo confirmó).
            None => Decision::NadaPendiente,
        },
        Err(err) => Decision::Fallo(err.to_string()),
    }
}

/// El watchdog completo.
///
/// `dormir` y `avisar` se inyectan para que los tests corran instantáneos y
/// deterministas, sin relojes de verdad. En producción son
/// `std::thread::sleep` y el emisor de eventos de Tauri.
///
/// **Un watchdog viejo no puede pisar un apply nuevo.** Si el usuario confirma y
/// vuelve a aplicar, el watchdog de la operación anterior se despierta, ve una
/// confirmación pendiente que **no venció** (es la nueva, recién creada) y se va
/// a dormir o abandona por tope de vueltas — nunca revierte el cambio nuevo.
/// Esa propiedad la da el manager al chequear el vencimiento, no un contador de
/// generación; está testeada abajo.
pub(super) fn correr<B, S>(
    manager: &Mutex<MonarchDisplayManager<B, S>>,
    plazo: Duration,
    dormir: &mut dyn FnMut(Duration),
    avisar: &mut dyn FnMut(Desenlace),
) where
    B: DisplayBackend,
    S: ConfigStore,
{
    let mut espera = plazo.saturating_add(MARGEN);
    let mut ultimo_fallo: Option<String> = None;

    for vuelta in 0..MAX_VUELTAS {
        dormir(espera);

        // El lock se toma y se suelta DENTRO de esta vuelta, en un scope corto:
        // nunca se sostiene mientras se duerme. (No hay `.await` de por medio;
        // esto corre en un hilo dedicado, no en el reactor de Tokio.)
        let decision = {
            let mut guard = match manager.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    // Con `panic = "abort"` un Mutex no puede envenenarse: el
                    // panic que lo envenenaría aborta el proceso antes. Si aun
                    // así pasara, se deja rastro y se sale — reintentar sobre un
                    // manager en estado desconocido es peor.
                    diagnostics::log("watchdog:mutex_envenenado:abandono");
                    avisar(Desenlace::NoPudoRevertir(
                        "el estado de displays quedó inconsistente".to_string(),
                    ));
                    return;
                }
            };
            decidir(&mut guard)
        };

        match decision {
            Decision::Revertido => {
                diagnostics::log(format!("watchdog:revertido:por_vencimiento:vuelta={vuelta}"));
                avisar(Desenlace::Revertido);
                return;
            }
            Decision::NadaPendiente => {
                avisar(Desenlace::NadaQueHacer);
                return;
            }
            Decision::Esperar(restante) => {
                espera = restante.max(ESPERA_MINIMA);
            }
            Decision::Fallo(err) => {
                diagnostics::log(format!(
                    "watchdog:rollback_rechazado:vuelta={vuelta}:{err} — reintentando"
                ));
                ultimo_fallo = Some(err);
                espera = ESPERA_TRAS_FALLO;
            }
        }
    }

    // Se acabaron las vueltas con algo todavía pendiente. Es el peor caso y no
    // se traga en silencio: el usuario tiene una pantalla en un estado que él no
    // confirmó y tiene que saberlo para poder revertir a mano.
    let motivo = ultimo_fallo
        .unwrap_or_else(|| "el plazo no llegó a vencer dentro del tope de intentos".to_string());
    diagnostics::log(format!("watchdog:AGOTADO:{motivo}"));
    avisar(Desenlace::NoPudoRevertir(motivo));
}

#[cfg(test)]
mod tests {
    use super::*;
    use monarch::{
        DisplayId, DisplayInfo, Layout, ManagerError, MemoryConfigStore, MockBackend, OutputConfig,
        Position, Resolution,
    };

    fn id(target: u32) -> DisplayId {
        DisplayId {
            adapter_luid: 1,
            target_id: target,
            edid_hash: Some(target as u64),
        }
    }

    fn salida(target: u32, enabled: bool, primary: bool, x: i32) -> OutputConfig {
        OutputConfig {
            display_id: id(target),
            enabled,
            position: Position { x, y: 0 },
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            refresh_rate_mhz: 60_000,
            primary,
        }
    }

    fn monitor(target: u32, active: bool, primary: bool) -> DisplayInfo {
        DisplayInfo {
            id: id(target),
            friendly_name: format!("Monitor {target}"),
            is_active: active,
            is_primary: primary,
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
            refresh_rate_mhz: 60_000,
        }
    }

    /// Dos monitores prendidos: el primario y la "TV". Es el escenario real del
    /// usuario, reducido a lo mínimo.
    fn manager_con_tv_prendida(
        plazo: Duration,
    ) -> Result<MonarchDisplayManager<MockBackend, MemoryConfigStore>, ManagerError> {
        let layout = Layout {
            outputs: vec![salida(1, true, true, 0), salida(2, true, false, 1920)],
        };
        let backend = MockBackend::new(vec![monitor(1, true, true), monitor(2, true, false)], layout)?;
        let mut manager = MonarchDisplayManager::new(backend, MemoryConfigStore::default())?;
        manager.set_confirmation_timeout(plazo);
        Ok(manager)
    }

    /// Un `dormir` que no duerme: los tests son instantáneos y deterministas.
    fn sin_dormir() -> impl FnMut(Duration) {
        |_| {}
    }

    // -------------------------------------------------------------------
    // EL test de la fase: si no confirmás, vuelve sola.
    // -------------------------------------------------------------------
    #[test]
    fn si_no_confirmas_el_watchdog_revierte_solo() {
        // Plazo cero ⇒ vence apenas se aplica. Es la forma de testear el
        // vencimiento sin esperar diez segundos de reloj.
        let mut manager = manager_con_tv_prendida(Duration::ZERO).expect("arma el manager");

        // El usuario apaga la TV (target 2).
        manager.toggle_display(&id(2)).expect("apaga la TV");
        assert!(manager.has_pending_confirmation(), "queda esperando confirmación");
        assert_eq!(
            manager.get_layout().expect("layout").enabled_output_count(),
            1,
            "la TV quedó apagada"
        );

        let manager = Mutex::new(manager);
        let mut desenlaces = Vec::new();
        correr(
            &manager,
            Duration::ZERO,
            &mut sin_dormir(),
            &mut |d| desenlaces.push(d),
        );

        assert_eq!(desenlaces, vec![Desenlace::Revertido]);

        let guard = manager.lock().expect("lock");
        assert!(!guard.has_pending_confirmation(), "ya no hay nada pendiente");
        assert_eq!(
            guard.get_layout().expect("layout").enabled_output_count(),
            2,
            "la TV volvió sola"
        );
    }

    #[test]
    fn si_confirmas_el_watchdog_no_toca_nada() {
        let mut manager = manager_con_tv_prendida(Duration::ZERO).expect("arma el manager");
        manager.toggle_display(&id(2)).expect("apaga la TV");
        manager.confirm_current_layout().expect("el usuario confirma");

        let manager = Mutex::new(manager);
        let mut desenlaces = Vec::new();
        correr(
            &manager,
            Duration::ZERO,
            &mut sin_dormir(),
            &mut |d| desenlaces.push(d),
        );

        assert_eq!(desenlaces, vec![Desenlace::NadaQueHacer]);
        let guard = manager.lock().expect("lock");
        assert_eq!(
            guard.get_layout().expect("layout").enabled_output_count(),
            1,
            "lo confirmado se respeta: la TV sigue apagada"
        );
    }

    #[test]
    fn un_watchdog_viejo_no_revierte_un_cambio_nuevo() {
        // Escenario: el usuario apaga la TV, confirma, y enseguida hace otro
        // cambio. El watchdog del PRIMER cambio se despierta tarde. No tiene que
        // tocar el segundo.
        let plazo_largo = Duration::from_secs(600);
        let mut manager = manager_con_tv_prendida(plazo_largo).expect("arma el manager");

        manager.toggle_display(&id(2)).expect("primer cambio");
        manager.confirm_current_layout().expect("confirma el primero");
        manager.toggle_display(&id(2)).expect("segundo cambio: la vuelve a prender");
        let antes = manager.get_layout().expect("layout");

        let manager = Mutex::new(manager);
        let mut desenlaces = Vec::new();
        // El watchdog viejo corre con el plazo viejo, ya vencido para él.
        correr(
            &manager,
            Duration::ZERO,
            &mut sin_dormir(),
            &mut |d| desenlaces.push(d),
        );

        // Se va sin poder revertir (correcto: no era suyo), pero NO tocó nada.
        assert_eq!(desenlaces.len(), 1);
        assert!(
            matches!(desenlaces[0], Desenlace::NoPudoRevertir(_)),
            "abandona por tope de vueltas, no revierte: {:?}",
            desenlaces[0]
        );
        let guard = manager.lock().expect("lock");
        assert_eq!(
            guard.get_layout().expect("layout"),
            antes,
            "el cambio nuevo quedó intacto"
        );
        assert!(
            guard.has_pending_confirmation(),
            "la confirmación del cambio nuevo sigue viva, esperando SU watchdog"
        );
    }

    #[test]
    fn mientras_no_vence_el_watchdog_espera_no_revierte() {
        let mut manager = manager_con_tv_prendida(Duration::from_secs(600)).expect("arma");
        manager.toggle_display(&id(2)).expect("apaga la TV");
        let apagada = manager.get_layout().expect("layout");

        let mut esperas = Vec::new();
        {
            let manager = Mutex::new(manager);
            let mut desenlaces = Vec::new();
            correr(
                &manager,
                Duration::from_secs(600),
                &mut |d| esperas.push(d),
                &mut |d| desenlaces.push(d),
            );
            assert!(matches!(desenlaces[0], Desenlace::NoPudoRevertir(_)));
            let guard = manager.lock().expect("lock");
            assert_eq!(
                guard.get_layout().expect("layout"),
                apagada,
                "no revirtió nada antes de tiempo"
            );
        }

        assert_eq!(esperas.len(), MAX_VUELTAS as usize, "durmió el tope de vueltas");
        assert!(
            esperas.iter().all(|d| *d >= ESPERA_MINIMA),
            "ninguna espera fue en vacío (giraría quemando CPU): {esperas:?}"
        );
    }

    #[test]
    fn la_primera_espera_incluye_el_margen_sobre_el_plazo() {
        // La carrera que el donante tenía abierta: dormir EXACTO el plazo.
        let mut manager = manager_con_tv_prendida(Duration::from_secs(600)).expect("arma");
        manager.toggle_display(&id(2)).expect("apaga la TV");

        let manager = Mutex::new(manager);
        let mut esperas = Vec::new();
        correr(
            &manager,
            Duration::from_secs(10),
            &mut |d| esperas.push(d),
            &mut |_| {},
        );

        assert_eq!(
            esperas.first().copied(),
            Some(Duration::from_secs(10) + MARGEN),
            "la primera espera tiene que pasarse del plazo, no quedarse justa"
        );
    }

    #[test]
    fn sin_nada_pendiente_se_va_en_la_primera() {
        let manager = Mutex::new(manager_con_tv_prendida(Duration::ZERO).expect("arma"));
        let mut desenlaces = Vec::new();
        let mut esperas = Vec::new();
        correr(
            &manager,
            Duration::ZERO,
            &mut |d| esperas.push(d),
            &mut |d| desenlaces.push(d),
        );
        assert_eq!(desenlaces, vec![Desenlace::NadaQueHacer]);
        assert_eq!(esperas.len(), 1, "una sola vuelta, no cinco");
    }

    #[test]
    fn el_ultimo_monitor_activo_no_se_puede_apagar() {
        // Guarda del manager, pero vale confirmarla: si esto se rompiera, un
        // toggle dejaría la máquina a ciegas y el watchdog sería el único
        // camino de vuelta.
        let mut manager = manager_con_tv_prendida(Duration::ZERO).expect("arma");
        manager.toggle_display(&id(2)).expect("apaga la TV");
        manager.confirm_current_layout().expect("confirma");

        let err = manager
            .toggle_display(&id(1))
            .expect_err("apagar el último activo tiene que fallar");
        assert!(
            matches!(err, ManagerError::Validation(_)),
            "esperaba un error de validación, vino {err:?}"
        );
        assert!(!manager.has_pending_confirmation(), "no quedó nada aplicado");
    }
}
