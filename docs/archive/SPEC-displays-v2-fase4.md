# ✅ IMPLEMENTADO 2026-07-27 (beta v1.5.0-beta.1) — SPEC: Displays v2 · Fase 4 — Perfiles como escenas (Millennium Clipboard)
Al aplicar un perfil, además de mover monitores (Fase 1/2) y cambiar el audio (Fase 3), disparar ACCIONES (abrir Steam Big Picture / un juego / Chrome con una cuenta+link, fijar volumen) y, al salir de la escena, correr su limpieza.
- Estado: **IMPLEMENTADO** — releaseado como prerelease `v1.5.0-beta.1`. Verificado local (28 tests vendor, harness Win/Linux, E2E frontend) + build CI verde. En hardware (Guido): volumen + Big Picture OK; **pendiente** probar Chrome (cuenta+link), la SALIDA (cerrar Big Picture) y el auto-revert-sin-acciones → luego release final `v1.5.0`. Ver `docs/DECISIONS.md` ADR-014.
- Fecha: 2026-07-27

## Por qué (el dolor)
Hoy un perfil es una "foto" de video + audio, pero no hace nada más. El caso real de Guido: usa los displays para **jugar y ver pelis en la TV**, y cada vez tiene que abrir a mano lo que va con esa configuración (Steam Big Picture, el navegador con su cuenta de pelis). La Fase 4 hace que el perfil sea la **escena completa**: aplicás "jugar en la tele" y se abre Steam en la TV solo; volvés a "trabajar" y se cierra lo que abriste. Norte concreto: **"jugar en la tele" → Big Picture (o un juego puntual) en la TV**, que cae solo en la tele porque el perfil la pone de primaria.

## Contexto del código (nombres REALES — verificar líneas, el código se movió desde Fase 3)

**Factibilidad:** ALTA y más simple que Fase 3. Lanzar procesos no necesita COM ni features nuevas del crate `windows` (`std::process::Command` es std); solo abrir URIs/`.lnk` conviene por `ShellExecuteW` (o `tauri-plugin-opener`). El estado y los enganches ya tienen molde en la Fase 3.

- **Qué es un perfil hoy**: `Profile { name, layout }` — crate vendorizado `monarch` (`src-tauri/vendor/monarch/src/model.rs`). Pura topología de video; sin concepto de acciones.
- **Dónde vive la metadata por perfil (patrón CLAVE)**: `AppSettings` (`vendor/monarch/src/model.rs:112`, `#[serde(default)]` a nivel struct) YA guarda cosas por NOMBRE de perfil: `profile_shortcuts: BTreeMap<String,String>` (model.rs:120) y `profile_audio: BTreeMap<String, AudioTarget>` (model.rs:126, `AudioTarget` en model.rs:105). Es exactamente el lugar/patrón para "metadata por perfil que no es topología de video": un mapa nombre→valor en `AppSettings`.
- **El enganche donde Fase 3 aplica el audio**: `aplicar_audio_de_perfil` (`mod.rs:1323`), llamado desde `cargar_perfil` (`mod.rs:944`) y `aplicar_perfil_directo` (`mod.rs:986`). **Estado paralelo en la capa `mod.rs`** (NO en el vendor): `audio_previo: Mutex<Option<…>>`, capturado al aplicar, consumido en `restaurar_audio_previo` (mod.rs:1384) en el revert, descartado en `olvidar_audio_previo` (mod.rs:1395) en el confirm. **ESTE es el molde para el estado paralelo de "escena activa".**
- **Dos vías de aplicar**: `cargar_perfil` (con red de auto-revert; tiene DOS ramas — `None` = no-op inmediato si el layout ya coincide, `Some(plazo)` = `aplicar_con_red` con cuenta regresiva) y `aplicar_perfil_directo` (commit inmediato; lo usan el **perfil de arranque** y los **atajos globales**).
- **El auto-revert**: `confirm` / `revert` / el watchdog (capa `mod.rs`) re-aplican el layout previo. El motor del vendor (`PendingConfirmation`, `confirm_current_layout`, `rollback_pending`) es **intocable**.
- **El embudo de settings (TRAMPA #1 heredada de Fase 3)**: `update_settings` (`vendor/monarch/src/manager.rs`) **NO copia el struct — lo RE-ARMA campo por campo**. Todo cambio de ajustes pasa por ahí (`guardar_ajustes`, `asignar/limpiar_atajo`, `borrar_perfil`, y los comandos de audio de Fase 3).
- **`ProfileView` + `listar_perfiles`** cruzan `profile_shortcuts`/`profile_audio` para mostrar lo guardado por perfil (molde a imitar para las acciones).
- **Frontend**: sub-pestaña PERFILES; `buildProfileItem` (main.js) pinta cada item (innerHTML estático + `setText`, nunca strings crudos del backend). Comandos existentes: `displays_save/load/delete/list_profile(s)`, `displays_set/clear_profile_shortcut`, `displays_set/clear_profile_audio` (Fase 3). El `invoke_handler` (lib.rs) los lista a todos.

## Decisiones tomadas con Guido (lluvia de ideas 2026-07-27)
1. **Escenas**: 2-3 fijas (jugar / ver / trabajar). **Cualquier perfil puede ser una escena** — no hay tipo aparte.
2. **Al volver**: la escena **cierra sola** lo que abrió (modelo "ida y vuelta"). "Volver" = aplicar otro perfil → corre la **SALIDA** de la escena anterior y después entra la nueva.
3. **Piezas de la fase**: lanzar app/URI/juego (core), **volumen por perfil**, **comando libre** (= lanzar con los campos a la vista).
4. **Control de TV / HDMI-CEC**: **DESCARTADO del todo.** La TV es una TCL con Google TV (caso frágil: ADB/Android TV, prender por WiFi dudoso, cambiar entrada no estándar). Guido la prende y elige el HDMI con el control; la escena hace todo lo del lado PC.
5. **"Ver pelis"** = abrir **Chrome con una cuenta específica** (`--profile-directory`) y un **link**; puede quedar abierta al volver (su salida vacía). El "comando libre" cubre esto sin código específico de Chrome.
6. **Reuso modular**: por **COPIA** en v1 (no "piezas por referencia"). Si molesta editar en varios lados, se migra después.

### Las 3 escenas de Guido, concretas
| Escena | Foto (ya existe) | Acciones de ENTRADA | Acciones de SALIDA |
|---|---|---|---|
| 🎮 Jugar en la tele | TV primaria + audio a la TV | volumen tele + `Lanzar` Big Picture **o** un juego | `Lanzar` cerrar Big Picture (asterisco, ver Riesgos) |
| 🍿 Ver pelis | TV primaria + audio a la TV | volumen tele + `Lanzar` Chrome (cuenta + link) | (vacía — la deja abierta) |
| 💻 Trabajar | monitores de laburo + audio parlantes | (vacía) | (vacía — al aplicarla dispara la SALIDA de la escena anterior) |

## AGREGA (lo nuevo)
- **Dato nuevo — `enum Accion`** (vendor `model.rs`, `Serialize/Deserialize` con tag serde para futuro-compat):
  - `Lanzar { destino: String, args: Vec<String> }` — `destino` = ruta a `.exe`, URI de protocolo (`steam://…`) o `.lnk`. Cubre Big Picture, juego, Chrome cuenta+link, Kodi/Plex, y el **"comando libre"** (es esta misma variante con los campos a la vista).
  - `Volumen { nivel: u8 }` — 0–100, sobre la salida de audio por default actual.
- **Dato nuevo — `PerfilAcciones { entrada: Vec<Accion>, salida: Vec<Accion> }`** (default = ambas listas vacías).
- **Dato nuevo — el mapa por perfil**: `AppSettings.profile_actions: BTreeMap<String, PerfilAcciones>`, `#[serde(default)]`, **en `AppSettings`** (mismo lugar/patrón que `profile_audio`). Sin entrada = perfil sin acciones = **como hoy**.
- **Estado paralelo nuevo (capa `mod.rs`, NO en el vendor)**: `escena_activa: Mutex<Option<String>>` — el nombre del perfil-escena puesto. **Molde**: el `audio_previo` de Fase 3. Nace `None` (al reiniciar la app no corre la limpieza de lo que hubiera quedado abierto — aceptado por Guido).
- **Backend — motor de acciones** (módulo nuevo, ej. `src-tauri/src/displays/acciones.rs`):
  - `ejecutar(&[Accion])` — corre en **orden**, **best-effort**, en `tokio::task::spawn_blocking`, **sin panics** (`panic=abort`). Cada fallo → `runtime_log::warn` y **sigue** con la siguiente.
  - `Lanzar`: **dispatch por tipo de destino** — `.exe` → `std::process::Command` con los **args EN LISTA** (sin shell, sin `cmd /c`) + `creation_flags(CREATE_NO_WINDOW)`; URI (contiene `://`) o `.lnk` → `ShellExecuteW` verbo `"open"` (o `tauri-plugin-opener`). **NUNCA** construir un string de shell ni concatenar (ver Riesgos). Nota Chrome: `.lnk` NO se puede lanzar por `Command` (CreateProcess solo corre `.exe`) — de ahí el dispatch.
  - `Volumen`: setear el nivel del endpoint por default actual (`IAudioEndpointVolume`, API documentada, misma familia COM que Fase 3 ya activó).
- **Backend — presets** (ayudan a rellenar un `Lanzar`; el dato guardado es un `Lanzar` común):
  - **Big Picture**: resolver `steam.exe` por registro (`winreg`: `HKCU\Software\Valve\Steam\SteamExe`; fallback `HKLM\…\WOW6432Node\Valve\Steam\InstallPath`) → `Lanzar { steam.exe, ["-start","steam://open/bigpicture"] }` (cubre Steam abierto o cerrado; `-fulldesktopres` opcional para TV 4K).
  - **Juego de Steam**: `["-start","steam://rungameid/<id>"]`.
  - **Cerrar Big Picture** (para SALIDA): `Lanzar` de `steam://close/bigpicture` — ⚠ no confirmado en Windows (Riesgos).
  - **Abrir link en Chrome**: `Lanzar { chrome.exe, ["--profile-directory=<carpeta>","--new-window","--start-fullscreen","<url>"] }`. `<carpeta>` = nombre de CARPETA del perfil (`Default`/`Profile N`, **no** el nombre visible); lo elige el usuario o se resuelve del `Local State` (deluxe, fuera de v1).
- **Backend — comandos Tauri nuevos**: `displays_set_profile_actions { name, entrada, salida }` y `displays_clear_profile_actions { name }` (molde exacto: `displays_set/clear_profile_audio`). Persisten en `profile_actions`.
- **Backend — enganche del ciclo de escena** en `cargar_perfil` / `aplicar_perfil_directo` / `confirm`:
  - **Regla dura**: las acciones (ENTRADA del nuevo perfil + SALIDA del anterior) corren **SOLO cuando el cambio de perfil queda COMMITTEADO**. En las vías inmediatas (`aplicar_perfil_directo` commit; rama `None` de `cargar_perfil`) corren ahí; en la vía con red (`Some(plazo)`) corren en **`confirm`**, NO al aplicar. Si el cambio se **auto-revierte**, **NO corre ninguna acción**.
  - Al commitear el cambio a perfil X: (1) si `escena_activa = Some(prev)` y `prev != X` → `ejecutar(prev.salida)`; (2) `ejecutar(X.entrada)`; (3) `escena_activa = Some(X)`. **Orden respecto al video/audio**: la foto y el audio primero (ya existen), **después** las acciones (para que Big Picture caiga en la TV ya primaria).
- **Frontend — editor de acciones por perfil**: en `buildProfileItem`, una sección **"Acciones"** con listas de ENTRADA y SALIDA (agregar / quitar / reordenar). Botones-preset (Big Picture · Juego · Abrir link en Chrome · Volumen · Comando libre) que arman una `Accion`. **Escapar** todo con `setText`/`textContent`. Invoca `displays_set_profile_actions`.

## MODIFICA (lo existente que se toca)
- **`AppSettings`** (`vendor/monarch/src/model.rs:112`): se agrega `profile_actions`. → Un `displays.json` viejo (sin el campo) DEBE leerse igual, por el `#[serde(default)]` de struct; el resto intacto. Sumar el campo a `impl Default`.
- **🔴 `update_settings`** (`vendor/monarch/src/manager.rs`, re-arma campo por campo — **TRAMPA #1**): DEBE copiar `profile_actions` tal cual del input (`profile_actions: settings.profile_actions`). **NUNCA** `Default::default()` / `BTreeMap::new()`: si se tapa así, **cada guardado de ajustes** (perfil de arranque, plazo, atajo, borrar perfil, audio) **borra todas las acciones en silencio**. Es la trampa #1 de ESTA fase también.
- **`cargar_perfil` / `aplicar_perfil_directo` / `confirm`** (`mod.rs`): enganchar el ciclo de escena (arriba). → El video (con y sin red) y el audio de Fase 3 siguen **igual**; las acciones son un paso adicional, best-effort, **gated a commit**. OJO las dos ramas de `cargar_perfil`: las acciones inmediatas van en la rama `None`, las diferidas a `confirm` en la rama `Some`.
- **`ProfileView` + `vista_de_perfil` + `listar_perfiles`** (`mod.rs`): sumar a `ProfileView` las acciones guardadas del perfil, cruzando `profile_actions` **igual que hoy con `profile_audio`**. → Sin esto, el editor no muestra lo que el perfil ya tiene.
- **`borrar_perfil`** (`mod.rs`) / **`displays_delete_profile`**: limpiar también la entrada en `profile_actions` (igual que atajo/audio); y si el perfil borrado era `escena_activa`, limpiar ese estado. → No dejar acciones huérfanas. (Pasa por `update_settings` → trampa #1.)
- **`Cargo.toml`** (si se lanza por `ShellExecuteW`): activar la feature `Win32_UI_Shell` del crate `windows` en `[target.'cfg(target_os="windows")'.dependencies]`. El volumen usa `Win32_Media_Audio` (ya activa por Fase 3). → **No** cambiar el pin `windows = "0.60"` ni las features ya activas.
- **El `invoke_handler`** (`lib.rs`): sumar los comandos nuevos a `generate_handler![…]`. → Los existentes quedan igual.

## NO SE TOCA (obligatoria — el seguro de no romper)
- **El struct `Profile`** (`model.rs`): **NO se le agrega ningún campo.** Las acciones viven en `AppSettings`. La topología de video del perfil queda idéntica byte a byte.
- **Los perfiles guardados** (`AppConfig.profiles`): ni se migran, ni se transforman, ni se renombran, ni se reordenan.
- **El motor CCD de video**, la **red de auto-revert de VIDEO**, el lienzo, el primario, el perfil de arranque, los atajos y las resoluciones — intactos. Las acciones se enganchan **DESPUÉS** del apply/confirm, sin tocar esa lógica ni el vendor (`PendingConfirmation`, `confirm_current_layout`, `rollback_pending`).
- **El audio por perfil de Fase 3** (`aplicar_audio_de_perfil`, `audio_previo`, los 3 roles, `IPolicyConfig`): intacto. El **volumen** es una acción NUEVA y aparte; **no reemplaza** el ruteo de salida de audio.
- **El núcleo de Millennium**: clipboard, discovery mDNS, servidor HTTPS/axum, transferencias, pinning de certificados — intacto. El formato del **hello UDP** y del JSON de **`/info`** — intacto.
- **El estilo neón** y el patrón push-based / diff incremental del frontend (`buildProfileItem` por diff).
- **CPU en reposo ~0%**: las acciones **NO** agregan timers ni polls; corren solo al aplicar/confirmar un perfil. En reposo, cero trabajo nuevo.
- **Android / no-Windows**: el motor de acciones va tras `#[cfg(target_os = "windows")]`; el editor de acciones NO se renderiza en `is-mobile`. El build de Android **NO se rompe**.

## Criterios de aceptación (verificables, regresión primero)
> **Vara de regresión**: el release actual **`v1.4.0`** (Displays v2 Fase 1+2+3).
1. **Regresión**: todo lo de NO SE TOCA sigue igual. Un `displays.json` de v1.4.0 (sin `profile_actions`) DEBE abrir sin romper, con **todos los perfiles sin acciones** y su video/audio/atajos/startup idénticos; auto-revert de video, audio de Fase 3 y transferencias del clipboard andan igual.
2. CUANDO Guido define acciones para un perfil, el sistema DEBE persistirlas en `profile_actions` y DEBEN reaparecer al reabrir la app. **Y DEBEN sobrevivir a** cambiar el perfil de arranque, borrar OTRO perfil, tocar un atajo o el audio (regresión de la **trampa #1** de `update_settings`).
3. CUANDO se aplica y **CONFIRMA** un perfil con acción "abrir Big Picture", Steam DEBE abrir en Big Picture en el **monitor primario (la TV)**, **después** de que la foto puso la TV primaria. (E2E en hardware.)
4. CUANDO se aplica un perfil con "abrir juego" (`steam://rungameid/<id>`), DEBE abrir ese juego.
5. CUANDO se aplica un perfil con "abrir Chrome (cuenta+link)", Chrome DEBE abrir con **esa** cuenta (`--profile-directory`) en el link. (Fullscreen es best-effort si ya había Chrome abierto en esa cuenta — ver Supuestos.)
6. CUANDO se pasa de una escena activa a OTRO perfil, la **SALIDA** de la escena anterior DEBE correr (ej. cerrar Big Picture) al commitear el nuevo.
7. SI el cambio de monitores se **AUTO-REVIERTE** (timeout o REVERTIR manual), **NINGUNA** acción de entrada/salida DEBE haber corrido (no se abre Steam si el video se revirtió).
8. CUANDO una acción falla (ruta inexistente, comando que no arranca), el sistema DEBE loguearlo y **seguir**; el cambio de monitores/audio **NO** se rompe.
9. Los argumentos se pasan de forma **segura** (en lista, sin shell): un destino con espacios/comillas **NO DEBE** poder inyectar un comando extra.
10. En **Android / no-Windows** el build NO se rompe, el editor de acciones NO aparece; y **CPU en reposo ~0%** (Task Manager) — las acciones no agregan timers.

## Migración de datos — tabla "esto cambio / esto preservo"
> Migración **aditiva y no destructiva**. Aprobar junto con el spec.

| Dato del usuario | Qué pasa |
|---|---|
| Perfiles existentes (`AppConfig.profiles`, cada `Profile{name,layout}`) | **SE PRESERVAN intactos.** `Profile` NO se toca — no se le suma ningún campo. |
| `displays.json` de v1.4.0 (sin `profile_actions`) | **Se lee igual.** El mapa `profile_actions` tiene `#[serde(default)]` → si no está, arranca **vacío** = todos los perfiles sin acciones. |
| Atajos, audio por perfil, ajustes, primario, perfil de arranque, resoluciones, fingerprints | **Intactos.** No se tocan. |
| Lo que se AGREGA al archivo | Un campo `profile_actions` dentro de `AppSettings` (mismo lugar que `profile_shortcuts`/`profile_audio`). Nace vacío. |
| Cuando Guido define acciones para un perfil | Se agrega/actualiza **solo** la entrada de ESE perfil. Ningún otro se toca. |
| Cuando Guido borra un perfil | Se limpia también su entrada de acciones (igual que atajo/audio). |
| Cuando Guido guarda CUALQUIER ajuste | Reescribe `displays.json` entero vía `update_settings`. **El peligro real NO es leer el archivo viejo (lo cubre `serde(default)`), es la REESCRITURA**: `update_settings` DEBE copiar `profile_actions` tal cual (trampa #1). Hecho eso, las acciones sobreviven cada guardado. |
| Downgrade (abrir un `displays.json` CON acciones en una app vieja y guardar) | **Límite conocido y aceptado**: la versión vieja ignora `profile_actions` al leer y lo pierde al reescribir. Sin campo de versión para detectarlo. Poco probable en uso personal offline; asterisco consciente, no bloquea. |

## Supuestos
- [ALTO] Las acciones se guardan **keyed por NOMBRE de perfil** (como atajos/audio). Renombrar (= borrar + recrear) no arrastra las acciones. Aceptable.
- [ALTO] Las acciones son **best-effort**: si un lanzamiento falla, el video/audio ya aplicados **NO** se revierten; se loguea.
- [ALTO] Las acciones corren **SOLO al commitear** el cambio (confirm o vía inmediata); si el video se auto-revierte, no corren. Es la decisión que evita "Steam abierto sobre un layout que se revirtió".
- [MEDIO] Cerrar Big Picture al volver usa `steam://close/bigpicture`, **no confirmado en Windows** (ver Riesgos). Si no anda, la salida de "jugar" queda como asterisco / plan B.
- [MEDIO] Chrome: `--profile-directory` recibe el **nombre de carpeta** (`Default`/`Profile N`). Si Chrome **ya está abierto** en esa cuenta, `--new-window`/`--start-fullscreen` se **ignoran** (abre pestaña normal) — fullscreen es best-effort. NO se usa `--user-data-dir` (perdería los logins de las cuentas).
- [MEDIO] **"Cerrar app por nombre de proceso" NO entra en v1** (footgun: matar `chrome.exe` cierra **todo** Chrome, incluido el de trabajo). La SALIDA se arma con acciones `Lanzar` (ej. `steam://close/bigpicture`). Ver FUERA de alcance.
- [BAJO] Volumen best-effort sobre la salida por default actual; si no hay salida, se loguea.

## Riesgos y decisiones ⚠️
- ⚠️ **`steam://close/bigpicture` NO confirmado en Windows** (solo Linux/SteamOS). Consecuencia: la salida "cerrar BP" podría no andar. **Plan B**: simular `Alt+Enter` (togglea) o dejar el cierre manual de esa app. **VERIFICAR E2E** antes de darlo por bueno (regla de evidencia del proyecto).
- ⚠️ **Comando libre = correr lo que el usuario ponga.** Mitigación: **args EN LISTA** (sin shell/`cmd`, sin concatenar) → sin inyección; es su propia máquina y su propio comando (sin atacante remoto: la app es solo-LAN y esto no se expone por red). Consecuencia si se hiciera mal (`cmd /c` + concatenación): inyección tipo BatBadBut. **Decisión: NUNCA shell.**
- ⚠️ **Acciones atadas al commit, no al apply.** Consecuencia si se hiciera al apply: Steam abierto aunque el video se revierta. **Decisión**: correrlas en `confirm` / vías inmediatas. Es el punto de integración **más delicado** — verificar el caso auto-revert (criterio 7).
- ⚠️ **Toca datos del usuario (`displays.json`).** Migración aditiva (tabla arriba). **Trampa de LECTURA**: el campo va en `AppSettings` (con `serde(default)` de struct), **NUNCA** en `Profile` (si no, un store viejo no deserializa → se pierden TODOS los perfiles). **Trampa de ESCRITURA (#1)**: `update_settings` re-arma campo por campo → debe copiar `profile_actions` (criterio 2).
- ⚠️ **Editar el vendor `monarch` (`AppSettings`)**: patrón ya establecido (shortcuts y audio viven ahí). El **motor de video del vendor NO se toca**, solo el struct de settings.
- ⚠️ **Steam ya no tiene Big Picture "clásico"** (removido ~2023); todo entra a la gamepad UI. `-start steam://open/bigpicture` cubre Steam abierto o cerrado; `-bigpicture` solo **NO** sirve si Steam ya está abierto.

## Faseado / cómo sigue
- Es una fase nueva de Displays v2 (post Fase 3). NO bloquea nada del núcleo Millennium.
- **FUERA de alcance (futuro, su propio spec/incremento)**:
  - **Cerrar app por proceso** (con el footgun de Chrome resuelto — ej. cerrar solo la ventana/proceso correcto).
  - **Desplegables inteligentes**: juegos de Steam instalados (parsear la biblioteca) y cuentas de Chrome (leer `Local State`) para elegir de una lista en vez de pegar URI/carpeta.
  - **Wallpaper por perfil** (la maquinaria `IDesktopWallpaper` ya está en `apply.rs`).
  - **Control de TV por red** (prender / cambiar entrada) — descartado en esta fase; si algún día, su propio spike contra la TCL Google TV.
  - **Piezas modulares por referencia** (definir "audio a la tele" una vez y reusar en varias escenas, sin copia).
  - **Acción "esperar X ms"** explícita entre pasos.
- **Para el `/cierre` del que la implemente**: verificar en hardware los criterios **3/5/6/7** (Steam en la TV, Chrome con la cuenta, salida de escena, y el auto-revert SIN acciones), **archivar** este spec (`docs/archive/` + "✅ IMPLEMENTADO <fecha>" en la línea 1), sacarlo del TODO, y apuntar el handoff al próximo candidato (fix de la caché del updater 🟠, o resolución por perfil en la UI).
