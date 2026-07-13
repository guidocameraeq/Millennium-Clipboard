# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-13 · **Último commit de código**: b6479c3. Los docs de cierre + el archivado van en commits aparte.

## Qué se hizo
- **Fase 0 de Windows COMPLETADA Y VERIFICADA** (spec archivado en `docs/archive/phase-0-stop-the-bleed.md`), un commit por Tarea:
  - **0.3** FX compositor-only (grid por `transform`, sin `backdrop-filter`/`mix-blend-mode`, gates + toggle VISUAL FX). (`88fd306`)
  - **0.1** Clipboard poller: thread dedicado, gate por peers + `GetClipboardSequenceNumber`, hash RGBA antes de encodear, texto >1 MB descartado. (`6fc8b78`)
  - **0.2** Logs: ring de 2000 en el frontend, emit IPC solo con panel abierto, dedup del poller + fix de overflow `u8`. (`fce0cb1`)
  - **0.4** `[profile.release]`. (`f4a7af5`) · **0.5** Autostart se re-registra al `.exe` actual. (`ae2d3af`) · **Extra** `get_settings` completo. (`80d0adb`)
- **Review adversarial multi-agente** (5 dimensiones × 2 verificadores): 9 defectos reales, **los 9 corregidos** (`bb49552`, `f1d2d58`, `4e02cc2`, `b6479c3`) — incluido el de privacidad (sync no filtra clipboard pre-opt-in) y el doble typewriter.
- **Verificación física hecha** (usuario + esta sesión): CPU casi nulo en reposo, sync E2E, FX/logs, y el **flujo de autostart end-to-end** (copia al escritorio → cierre → arranque desde ahí → heal reescribe la entrada `Run` → arranque limpio sin crash).

## Estado
- Branch `main`. Build verde por máquina (check/clippy/test/node) · `.exe` release **9.8 MB**.
- **La app corriendo ahora es la copia del escritorio**: `C:\Users\clientes\OneDrive\Desktop eQ\Millennium Clipboard.exe` (PID cambia). La entrada de autostart apunta a esa ruta.
- Fase 0 **archivada** en `docs/archive/`. Verificación física: **OK**.

## En curso
- Nada. Fase 0 cerrada de verdad.

## Próximo paso CONCRETO
Arrancar la **Fase 1 de Windows — discovery** (`docs/remediation/windows/phase-1-discovery.md`) en un **chat nuevo** con `/inicio`. Es riesgo medio (toca lógica de red viva): testear con 2 dispositivos observando el panel de LOG (que la Fase 0 ya dejó acotado). Cuidar de no romper el hello UDP ni el JSON de `/info`.

## Bloqueos
- **Android**: decisión estratégica previa pendiente (núcleo headless vs foreground-only, `docs/remediation/android/SPEC.md`). No arrancar Android sin decidirla.

## Pendiente derivado (no urgente)
- **Autostart sin comillas** (va a la Fase 3 seguridad): la entrada `HKCU\...\Run` que escribe `tauri-plugin-autostart` no lleva comillas → *unquoted path* (CWE-428). Con la ruta del usuario (`OneDrive\Desktop eQ\Millennium Clipboard.exe`, con espacios) Windows la resuelve igual porque los paths intermedios no existen, pero conviene reescribirla con comillas. Ya anotado en TODO.

## Contexto que no está en otro doc
- **Divergencia con el spec (0.1)**: se usó `extern "system"` directo a user32 para `GetClipboardSequenceNumber` en vez de sumar la crate `windows` → mismo fix, cero dep nueva.
- **Escritorio del usuario**: está en OneDrive con nombre `Desktop eQ` (con espacio). Ojo con eso al armar rutas.
- **Entorno**: PowerShell 5.1 rompe los `git commit -m` con comillas dobles; usar `git commit -F -` con heredoc desde el Bash tool.
