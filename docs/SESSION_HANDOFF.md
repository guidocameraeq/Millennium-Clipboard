# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-27 (tarde) · **Branch**: `feat/displays-v2-fase4` (pusheado; NO mergeado a `main`) · **Working tree**: limpio tras el commit de este cierre · **Último commit**: `docs: cierre de sesión 2026-07-27 — Fase 4 implementada + beta v1.5.0-beta.1`

## En una línea

**Displays v2 Fase 4 "perfiles como escenas" — IMPLEMENTADA y releaseada como prerelease `v1.5.0-beta.1`.** Al aplicar un perfil, además de mover monitores (F1/2) y rutear el audio (F3), dispara ACCIONES de entrada (lanzar Big Picture / juego / Chrome cuenta+link, fijar volumen) y corre la SALIDA de la escena anterior al cambiar de perfil. **Verificada en hardware PARCIALMENTE por Guido: volumen + Big Picture OK.** Falta probar en hardware: Chrome (cuenta+link), la salida de escena (cerrar Big Picture) y el auto-revert-sin-acciones. **Próximo**: Guido termina de probar la beta → si OK, release final `v1.5.0`.

## Lo que se hizo esta sesión

- **Construida la Fase 4 entera** siguiendo `SPEC-displays-v2-fase4.md` (ahora archivado). Backend Rust + frontend, calcado del molde de Fase 3:
  - **Dato** (vendor `monarch`): `enum Accion { Lanzar { destino, args }, Volumen { nivel } }` (tag serde `"tipo"`, `snake_case`), `PerfilAcciones { entrada, salida }`, y el mapa `AppSettings.profile_actions` (side-map por nombre de perfil, `#[serde(default)]` de struct → compat con stores viejos). `update_settings` copia el campo tal cual (**trampa #1**) + 3 tests de regresión nuevos.
  - **Motor** (`src-tauri/src/displays/acciones.rs`, NUEVO): `ejecutar(&[Accion])` best-effort, sin panics. `Lanzar` con **dispatch por tipo**: `.exe` + args **EN LISTA** por `std::process::Command` (`CREATE_NO_WINDOW` + resolución por App Paths del registro para nombres pelados tipo `chrome.exe`); URI (`steam://…`) o `.lnk` por `ShellExecuteW` verbo `"open"`. `Volumen` por `IAudioEndpointVolume` sobre la salida por default. Sin shell → sin inyección.
  - **Ciclo de escena** (`mod.rs`): `escena_activa` + `escena_pendiente` (2 slots, molde `audio_previo`). Las acciones corren **SOLO al commitear** el cambio (vías inmediatas o `confirm`); si el video se auto-revierte (timeout/revert/no-pudo), **no corre ninguna**. Enganchado en `cargar_perfil` (ramas None/Some), `aplicar_perfil_directo` (None/Some), `confirm`, `revert` y `reportar_desenlace`. `borrar_perfil` limpia `profile_actions` y olvida la escena.
  - **Comandos**: `displays_set/clear_profile_actions`; `ProfileView` expone las acciones. `Cargo.toml`: feature `Win32_Media_Audio_Endpoints` (volumen).
  - **Frontend** (`main.js` + `styles.css`): editor de escena por perfil (botón `🎬 escena`, columnas Al entrar/Al salir con chips, presets Big Picture/Juego/Chrome/Volumen/Cerrar BP/Comando libre, reordenar ↑↓ / quitar ✕), todo por `textContent`. Oculto en Android.
- **Release**: 2 commits (feat + bump `v1.5.0-beta.1`) → tag → `release.yml` compiló el `.exe` en CI (**verde**) y publicó la **prerelease** con `.exe` + digest sha256.

## En qué estado quedó

- **Verificación local (fuerte)**: 28 tests del vendor `monarch` verdes (incl. trampa #1); el **motor de displays entero type-checkeó verde** en un crate scratch `#[path]` (ADR del gate local) en **Windows Y Linux** (sin fugas de `cfg` → Android no se rompe); `acciones.rs` verificado en un crate scratch Win32 aparte (firmas reales de `windows 0.60`); **frontend probado E2E con Playwright** (agregar/quitar/reordenar, forms de Volumen y Chrome, y **escaping** — un `<img onerror>` no se ejecutó, todo por `textContent`); `node --check` verde.
- **Compilación real**: el **build de CI de `v1.5.0-beta.1` salió verde** (compiló el crate entero, incluido `lib.rs`, y generó el `.exe`). Esa es la prueba de "compila" (local está roto por `dlltool`, como siempre).
- **Hardware**: Guido probó **volumen + abrir Big Picture → OK**. El resto sin probar aún.

## Lo que quedó en curso / próximo paso CONCRETO (al retomar)

1. **Guido termina de probar la beta `v1.5.0-beta.1`** (por el updater): criterios de hardware que faltan — (3) Big Picture cae **en la TV** (primaria); (5) **Chrome con esa cuenta** + link; (6) la **SALIDA** cierra Big Picture al volver (⚠ `steam://close/bigpicture` NO confirmado en Windows — si no cierra, es el asterisco conocido, plan B `Alt+Enter` o cierre manual); (7) aplicar un perfil y **dejar que se auto-revierta** → **no** se abre nada.
2. **Si la beta pasa → release FINAL `v1.5.0`**: bump a `1.5.0` (sin sufijo) en `Cargo.toml` + `tauri.conf.json` + `Cargo.lock`, commit, tag `v1.5.0` → la landing/README lo empieza a servir. Después **mergear `feat/displays-v2-fase4` a `main`** (FF, como Fase 3).
3. **Review adversarial completo**: el sweep de 5 frentes se cortó por **límite de sesión** (no corrió). La parte crítica (gating de la escena) la revisé a mano y quedó sólida (no se pueden apilar 2 confirmaciones ⇒ `escena_pendiente` no puede quedar colgada). Correr el sweep o `/code-review ultra` en un chat nuevo si se quiere el belt-and-suspenders.

## Bloqueos

- Ninguno.

## Archivos tocados (todo commiteado en la rama)

`src-tauri/vendor/monarch/src/{model.rs,manager.rs,lib.rs}` · `src-tauri/Cargo.toml` · `src-tauri/Cargo.lock` · `src-tauri/tauri.conf.json` · `src-tauri/src/displays/acciones.rs` (nuevo) · `src-tauri/src/displays/mod.rs` · `src-tauri/src/lib.rs` · `src/main.js` · `src/styles.css` · docs de este cierre.

## Contexto que no está en otros docs

- **Decisiones técnicas que tomé al construir** (mías, por la regla de que las técnicas las decido yo; ver DECISIONS ADR-014):
  - Big Picture / juego / cerrar-BP van por **URI `steam://…`** directo (más simple, anda con Steam abierto o cerrado), NO por `steam.exe -start` resuelto del registro como sugería el spec.
  - Lanzamiento por **`ShellExecuteW` (crate `windows`)**, NO `tauri-plugin-opener` — para mantener el motor de displays **libre de Tauri** (así se type-checkea local en el scratch, regla del ADR del gate).
  - Chrome se guarda como `Lanzar { "chrome.exe", […] }` y el motor resuelve la ruta por **App Paths** del registro en tiempo de ejecución (así el preset anda sin pedir la ruta completa).
- **`acciones.rs` usa `winreg`** (para App Paths) → el gate local (crate scratch) ahora **incluye winreg** en el bloque windows. La nota vieja de DECISIONS que decía "sacar winreg del scratch" quedó **actualizada** (hoy compila con `winreg 0.55` → `windows-sys 0.59`, sin `dlltool`).
- **El perfil de arranque / los atajos corren la ENTRADA de su escena** al aplicarse (vía `aplicar_perfil_directo`). O sea: si Guido pone "jugar" como perfil de arranque, Big Picture se abre al bootear. La SALIDA NO corre al arrancar (`escena_activa` nace `None`) — decisión del spec.
