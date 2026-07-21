# SPEC: Displays v2 · Fase 1 — Perfiles con superpoderes (Millennium Clipboard)
Exponer lo que el motor de Monarch ya soporta: elegir el monitor primario, aplicar un perfil al iniciar, atajos de teclado por perfil, y un botón para actualizar un perfil.
- Estado: READY
- Fecha: 2026-07-21

> **Este spec cubre la FASE 1.** El programa "Displays v2" tiene más fases (rediseño, audio por perfil, resolución por perfil); cada una lleva su propio spec cuando toque (ver "Faseado"). El ejecutor construye SOLO lo de este spec.

## Por qué (el dolor)
La Fase 3 dejó displays andando (ver/attach-detach, perfiles, ajustes, watcher, lienzo), pero Guido esperaba cosas que usaba en Monarch y no están: no puede **elegir el primario** al armar un perfil, no hay **atajos** para aplicar un perfil (importante para él), y falta **aplicar un perfil al encender la PC** (su caso: dejó la TV prendida → al bootear le pone cierto perfil solo). Además, actualizar un perfil obliga a un baile de borrar/recargar.

## Contexto del código (nombres REALES — se construyó en la Fase 3)
- **Motor**: `src-tauri/src/displays/` (CCD portado de Monarch) + comandos en `src-tauri/src/lib.rs` + UI en `src/index.html` / `src/main.js` / `src/styles.css` (modal `#displays-modal` con pestañas LISTA/PERFILES/AJUSTES/LIENZO).
- **Estado Tauri**: `DisplaysState(Arc<Interno>)` con `manager: Mutex<MonarchDisplayManager<SystemDisplayBackend, MillenniumConfigStore>>`. Comandos hoy: `displays_get_snapshot / toggle / confirm / revert / list_profiles / save_profile / load_profile / delete_profile / get_settings / update_settings / apply_layout`. La red compartida `aplicar_con_red(plazo)` arma el watchdog tras un apply de layout (la usan cargar-perfil y el lienzo).
- **El crate puro `vendor/monarch` YA soporta lo de esta fase; NO hay que tocarlo** (ADR-001):
  - `OutputConfig` (dentro del `Layout` de cada `Profile`) tiene **`primary`**, `resolution`, `refresh_rate_mhz`, `position`, `enabled`. O sea el perfil ya guarda el primario; la UI no deja elegirlo.
  - `AppSettings` tiene: `revert_timeout_secs` (lo único expuesto hoy), `start_with_windows`, **`startup_profile_name: Option<String>`**, **`global_shortcuts_enabled: bool`**, `profile_shortcut_base`, `display_toggle_shortcut_base`, **`profile_shortcuts: BTreeMap<String,String>`**, `display_toggle_shortcuts`. Todo se persiste ya; nadie lo usa. `update_settings(AppSettings)` reconstruye todo el settings (hay que preservar lo no tocado — el patrón ya está en `guardar_ajustes`).
  - Métodos del manager disponibles: `get_layout`, `apply_layout`, `apply_profile`, `save_profile`, `confirm_current_layout`, `settings`, `update_settings`, `list_profiles`, `delete_profile`. **No** hay glue de atajos (eso vivía en `app/` de Monarch y no viajó).
- **Store**: `MillenniumConfigStore` → `%APPDATA%/com.guidocameraeq.millennium/displays.json` (atómico). Guido ya tiene al menos un perfil guardado (probó la Fase 3).
- **Plugins Tauri**: `autostart` **YA cableado** (la app arranca con `--autostart` y se esconde en la tray — ver `lib.rs` ~1442 y el chequeo `--autostart` ~1531). **`tauri-plugin-global-shortcut` NO está** (hay que sumarlo). Ya existe el comando `set_start_with_windows`.
- **Restricciones** (CLAUDE.md + DECISIONS ADR-001..013): `panic=abort` (cero unwrap/expect en apply), doctrina CCD (verificar re-enumerando), store atómico, vendor por copia, y el gate local del crate scratch (2 ramas de cfg) + `displays-tests`.

## AGREGA (lo nuevo)
- **Hacer primario un monitor** — botón "★ primario" por monitor activo en la pestaña LISTA. Es un cambio en vivo → **pasa por la red** (`aplicar_con_red`, cuenta regresiva) como el detach/lienzo. Comando nuevo `displays_set_primary(display_id)`: lee el layout, marca ese output `primary=true` (los demás false) y lo re-ancla en `(0,0)` (corriendo el resto), y aplica por la red. Al guardar un perfil después, el primario queda capturado (ya lo captura del layout actual).
- **Aplicar un perfil al iniciar (startup profile)** — en AJUSTES, un selector "Aplicar al encender" con la lista de perfiles (o "ninguno"). Se guarda en `startup_profile_name` (extender `SettingsView` + `leer/guardar_ajustes`). Al arrancar con `--autostart`, aplicar ese perfil **directo, sin la red**: `apply_profile(name)` seguido de `confirm_current_layout()` (commit inmediato, sin pending); si el layout ya coincide, `apply_profile` es no-op. Corre en `spawn_blocking`, **no-fatal** (si falla, se loguea y la app sigue).
- **Atajos de teclado por perfil** — sumar `tauri-plugin-global-shortcut = "2"` (solo desktop/windows, gateado) + registrarlo. En la pestaña PERFILES, por fila, asignar/limpiar una combinación (comandos `displays_set_profile_shortcut(name, accel)` / `displays_clear_profile_shortcut(name)`, que persisten en `profile_shortcuts`). En AJUSTES, un interruptor general (`global_shortcuts_enabled`). Al arrancar (y al cambiar), registrar los hotkeys globales; al dispararse uno, aplicar ese perfil **directo, sin la red** (misma vía que el startup). Registro que falla (combinación en uso por otro programa) → se avisa y no se registra, sin romper.
- **Botón "actualizar perfil"** — por fila en PERFILES, "↻ actualizar", que pisa ese perfil con el layout actual. Reusa `save_profile(name)` + el banner de confirmación que ya existe (es dato del usuario → confirma antes de pisar).

## MODIFICA (lo existente que se toca — con su efecto a cuidar)
- **`SettingsView` + `leer_ajustes`/`guardar_ajustes`** (`displays/mod.rs`): sumar `startup_profile_name` y `global_shortcuts_enabled`. → `update_settings` reconstruye TODO el `AppSettings`; hay que seguir **preservando** los campos no tocados (el patrón de "leer actual, cambiar solo lo mío, reescribir" ya está — extenderlo, no romperlo).
- **`setup()` en `lib.rs`** (bloque `#[cfg(target_os="windows")]` del displays): tras `displays::init`, si hay `startup_profile_name` y el launch es `--autostart`, aplicar el startup profile. → No romper el arranque no-Windows/Android (gateado) ni bloquear el boot (spawn_blocking, no-fatal). Registrar también los hotkeys de perfiles acá (o en `init`).
- **`generate_handler!`** (`lib.rs`): sumar los comandos nuevos (`displays_set_primary`, `displays_set_profile_shortcut`, `displays_clear_profile_shortcut`), sin gatear la entrada (patrón actual). → El resto de comandos sigue igual.
- **`displays_delete_profile`** (y un futuro rename): al borrar un perfil, **limpiar su atajo** de `profile_shortcuts` (queda huérfano si no). → Un borrado hoy no toca settings; sumar ese cleanup.
- **Frontend** (`index.html`/`main.js`/`styles.css`): botón primario en LISTA, selector startup + toggle atajos en AJUSTES, asignar-atajo + botón actualizar en PERFILES. → El HUD, el modal y las 4 pestañas actuales siguen igual; render por diff + escaping por `textContent`.
- **`Cargo.toml`**: `tauri-plugin-global-shortcut = "2"` bajo el target-table desktop/windows. → Android NO debe verlo.

## NO SE TOCA (obligatoria — el seguro de no romper)
- **El núcleo de Millennium**: clipboard, discovery mDNS, servidor HTTP/axum, transferencias, pinning de certificados — intactos.
- **La red de auto-rollback del detach manual (LISTA) y del lienzo** — sigue igual. Lo "directo, sin red" es SOLO para startup y atajos (aplicados sin nadie mirando).
- **Los perfiles que Guido ya guardó** — NO se migran, renombran ni transforman. El formato (`AppConfig`/`AppSettings`) ya tiene lugar para los campos nuevos (serde default), así que leer su `displays.json` viejo es forward-compatible. **Sin tabla de migración porque no hay nada que transformar.**
- **El vendor `monarch`** — no se modifica; todo se hace en la glue de Millennium con la API que el manager ya expone.
- **CPU en reposo ~0%** — los atajos son por evento del plugin, no poll.
- **El motor CCD (apply/topology/enumerate/watchdog)** y las Fases 1–3 que ya andan.

## Criterios de aceptación (verificables, regresión primero)
1. **Regresión**: todo lo de NO SE TOCA sigue igual — clipboard/transferencias andan, el detach manual y el lienzo conservan su cuenta regresiva, y los perfiles ya guardados se leen sin cambios.
2. CUANDO Guido aprieta "★ primario" en un monitor activo, el sistema DEBE hacerlo primario (verificado re-enumerando), pasando por la cuenta regresiva; y al guardar el perfil, ese primario DEBE quedar en el perfil.
3. CUANDO hay un startup profile elegido y la PC arranca con la app en autostart, el sistema DEBE aplicar ese perfil **directo (sin cuenta regresiva)**; y si el layout ya coincide, NO DEBE hacer nada.
4. CUANDO Guido asigna un atajo a un perfil y lo aprieta con la app en la tray, el sistema DEBE aplicar ese perfil **directo**.
5. SI el interruptor general de atajos está apagado, ENTONCES ningún atajo DEBE disparar.
6. CUANDO Guido usa "↻ actualizar" en un perfil, el sistema DEBE pisar ese perfil con el layout actual, previa confirmación.
7. SI una combinación de atajo ya la usa otro programa, ENTONCES el sistema DEBE avisar y NO registrar ese atajo, sin romperse.
8. Los caminos de apply nuevos (primario, startup, atajo) DEBEN tener **cero unwrap/expect** (panic=abort), y la app DEBE arrancar igual en no-Windows/Android (nada de esto aparece ni rompe el build por `cfg`).

## Supuestos
- [ALTO] Los atajos son **globales** (system-wide, vía `tauri-plugin-global-shortcut`), no solo-con-foco. Si Guido los quería solo con la app enfocada, cambia el enfoque.
- [BAJO] El startup profile se aplica en el launch con `--autostart`; sin autostart no se dispara al bootear (es lo esperable).
- [BAJO] "Hacer primario" vive en la LISTA en la v1; sumar el mismo gesto al lienzo (click en un monitor → primario) queda para después.
- [BAJO] El atajo se aplica directo también si la ventana está visible (no solo en tray) — es coherente con "lo pediste a propósito".

## Riesgos y decisiones ⚠️
- ⚠️ **Startup y atajos aplican SIN la red** (decisión de Guido, 2026-07-21). Consecuencia: un perfil malo aplicado así **no vuelve solo**; se arregla a mano o con otro atajo. El detach manual y el lienzo mantienen su red. Aceptado.
- ⚠️ **Plugin nuevo** (`global-shortcut`): suma superficie de dependencias; va gateado desktop/windows para no tocar el build de Android (mismo cuidado que `windows 0.60`).
- ⚠️ **Datos del usuario**: los perfiles ya guardados NO se migran (el schema ya los acomoda). Los atajos se guardan **por nombre de perfil** → borrar (o renombrar) un perfil debe limpiar su atajo, o queda un hotkey apuntando a un perfil que no existe.
- ⚠️ **No se modifica el vendor `monarch`** (ADR-001): primario, startup y atajos se hacen todos en la glue de Millennium sobre `get_layout`/`apply_layout`/`apply_profile`/`confirm_current_layout`/`update_settings`. Si en el camino apareciera algo que EXIGE tocar el crate puro, frenar y re-evaluar (es paso deliberado con re-sync manual).

## Faseado
- **Fase 1 (este spec, READY)** — Perfiles con superpoderes: primario, startup, atajos, botón actualizar.
- **Fase 2** — Rediseño: sacar displays del pop-up a full-screen, app en dos secciones (archivos/clipboard vs displays). Toca el shell compartido con el clipboard → su propio spec con NO SE TOCA cuidado.
- **Fase 3** — Audio por perfil: al aplicar un perfil, cambiar el output de audio por default de Windows. Net-new, requiere **investigación** (API tipo `IPolicyConfig`) + extender qué guarda el perfil (dato del usuario → migración). Su propio spec, arrancando por un spike de investigación.
- **Más adelante** — Resolución/refresh por perfil (el `OutputConfig` ya lo guarda; falta capturarlo/editarlo en la UI).
