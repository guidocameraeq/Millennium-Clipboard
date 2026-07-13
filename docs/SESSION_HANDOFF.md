# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-13 · **Último commit**: b6479c3 (fase 0 + fixes del review). Los docs de este cierre quedan en un commit aparte.

## Qué se hizo
- **Fase 0 de Windows implementada de punta a punta** (spec `docs/remediation/windows/phase-0-stop-the-bleed.md`), un commit por Tarea:
  - **0.3** FX compositor-only: grid por `transform`, sin `backdrop-filter`/`mix-blend-mode`, animaciones gated por `prefers-reduced-motion` + `fx-paused` (ventana oculta) + toggle **VISUAL FX** en Settings. (`88fd306`)
  - **0.1** Clipboard poller: thread dedicado con un handle de arboard, gate por peers + `GetClipboardSequenceNumber` (FFI a user32, sin dep nueva) + hash RGBA antes de encodear; texto >1 MB se descarta. (`6fc8b78`)
  - **0.2** Logs: ring de 2000 líneas en el frontend, emit IPC solo con panel abierto, dedup del poller de discovery; de paso **fix de overflow** del contador `u8` de failures. (`fce0cb1`)
  - **0.4** `[profile.release]` (lto/strip/panic=abort/opt-level=s). (`f4a7af5`)
  - **0.5** Autostart se re-registra al `.exe` actual en cada arranque. (`ae2d3af`)
  - **Extra**: `get_settings` devolvía 2 de 5 campos → los toggles de Settings abrían en su default. (`80d0adb`)
- **Review adversarial multi-agente** sobre el diff completo (5 dimensiones × 2 verificadores escépticos): 9 defectos reales confirmados, 3 refutados. **Los 9 corregidos** (`bb49552`, `f1d2d58`, `4e02cc2`, `b6479c3`), destacando el de **privacidad** (activar sync retransmitía clipboard pre-opt-in) y el doble arranque del typewriter.

## Estado
- Branch `main` (rama única). Working tree: solo los docs de este cierre.
- **Build verde por máquina**: `cargo check` + `cargo clippy` (12 warnings, todos preexistentes, 0 nuevos) + `cargo test` (1/1) + `node --check`. `.exe` release = **9.8 MB** (antes ~25 MB).
- **NO VERIFICADO en runtime**: nada de lo físico (CPU/RAM, autostart real, sync E2E) fue medido — lo mira el usuario.

## En curso
- Nada a medio hacer. Build release final re-corriendo en background para reconfirmar tras los fixes (el previo dio 9.8 MB verde).

## Próximo paso CONCRETO
**Verificar la Fase 0 en físico** (es criterio de aceptación del spec, y el usuario pidió mirarlo él). Pasos:
1. **CPU/RAM en reposo**: abrir la app, copiar una imagen grande al portapapeles SIN peers con clipboard-sync, minimizar a bandeja. Task Manager: `millennium-clipboard.exe` debe quedar ~0% CPU y RAM estable. Comparar contra el consumo de antes.
2. **Autostart**: con START WITH WINDOWS en ON, reiniciar la app; `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"` → la entrada de Millennium debe apuntar al `.exe` actual, no al path viejo de v0.8.1. En el log: `[autostart] re-registered to current exe path`.
3. **FX**: minimizar → el repaint cae (clase `fx-paused` en `<html>`); toggle VISUAL FX off → grid/scanline desaparecen y persisten tras recargar.
4. **Sync E2E** (2 máquinas): habilitar clipboard-sync mutuo, copiar texto e imagen → se propagan. Copiar texto >1 MB → en el log `[clipboard] text too large … skipped`, no se manda como imagen.

Si todo pasa: marcar la línea 1 de `phase-0-stop-the-bleed.md` como VERIFICADA y moverla a `docs/archive/`, después arrancar la **Fase 1 (discovery)** en un chat nuevo.

## Bloqueos
- **Android** sigue con su decisión estratégica previa pendiente (núcleo headless vs foreground-only, `docs/remediation/android/SPEC.md`). No arrancar Android sin decidirla.

## Archivos tocados (código)
- `src-tauri/src/lib.rs` (poller, autostart heal, comando `set_log_panel_open`, `get_settings`, test), `src-tauri/src/discovery.rs` (dedup + saturating_add), `src-tauri/src/runtime_log.rs` (gate del emit), `src-tauri/src/settings.rs` (`loaded_from_corrupt`), `src-tauri/Cargo.toml` (`[profile.release]`), `src/main.js`, `src/styles.css`, `src/index.html`.

## Contexto que no está en otro doc
- **Divergencia con el spec (0.1)**: el spec pedía sumar la crate `windows` para `GetClipboardSequenceNumber`; se usó un `extern "system"` directo a user32 → mismo fix, cero dep nueva.
- **El poller de clipboard sin peers**: en Windows el gate por secuencia mantiene el tracking al día aunque no haya peers, así que NO se filtra contenido viejo al activar sync. En Linux/Mac (no son targets shippeados) ese gating fino no aplica — irrelevante hoy.
- **Entorno**: PowerShell 5.1 rompe los `git commit -m` con comillas dobles en el mensaje (las interpreta). Usar `git commit -F -` con heredoc desde el Bash tool.
- La `phase-0` NO se archivó a propósito: falta la verificación física del usuario.
