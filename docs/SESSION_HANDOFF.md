# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-13 · **Último commit de código**: `bffef4c`. Los docs de cierre + el archivado van en commits aparte.

## Qué se hizo
- **Fase 2 de Windows (correctness y seguridad de datos) IMPLEMENTADA** (spec archivado en `docs/archive/phase-2-correctness.md`), un commit por Tarea (2.2 dividida en sub-bugs):
  - **2.1** Módulo nuevo `json_store.rs` — `JsonStore<T>` genérico con **escritura atómica** (`.tmp` + `fs::rename`) y **backup-on-corrupt** (`<file>.corrupt` + log `ERR`, nunca más `unwrap_or_default()` silencioso). Los 6 stores delegan el I/O ahí conservando firmas públicas, nombres de archivo y formato. (`af7d56c`)
  - **2.2.a–f** Los 6 bugs de UI en `main.js`: normalizar `status` + try/catch por peer (`5c38836`); `setStatus` con prioridad/TTL (`9dbe0d5`); `TARGET LOST` sin re-apuntar solo (`5b7043a`); texto entrante en superficie propia `#incoming-toast` (`d2692a8`); barra RX separada de TX keyeada por `sessionId` (`b391c56`); rename inline sobrevive `peers-changed` (`f16de12`).
  - **2.3** Zombie-killer: mata por dueño del puerto 53319 (solo si es nuestro) + ambos nombres de deploy, skip en dev (`MILLENNIUM_INSTANCE`). (`55ce45e`)
  - **2.4** Update swap con reintentos (10×) + marcador `millennium-update-failed.txt` que la app avisa al arranque. (`54b4ee4`)
- **Review adversarial multi-agente** (5 dimensiones × 2 escépticos): 6 hallazgos → **3 confirmados + 1 endurecimiento aplicados** (`bffef4c`), 3 refutados. Confirmados: TTL de `setStatus` suprimía la confirmación tras una acción del usuario (→ `{force}`); zombie-killer sin chequeo de propiedad del puerto (→ solo mata si el owner es nuestro); aviso de update fallido podía perderse (→ modelo pull, comando `take_update_failure`). Endurecimiento: `JsonStore::update` mantiene el `Mutex` a través del persist.

## Estado
- Branch `main`. **Build verde por máquina**: `cargo check` OK, `cargo clippy` 13 warnings (= baseline, 0 nuevos, 0 en archivos de la fase), `cargo test --lib` 7/7, `node --check src/main.js` OK.
- **Round-trip sobre los 6 JSON reales del usuario: OK** — harness aislado sin Tauri (`scratchpad/jsonstore_verify`, `include!` del `json_store.rs` vivo): los 6 cargan→guardan→cargan idéntico. `settings` 204→178 B: cae **solo** `registerSendTo` (vestigio de la feature Send To v0.10.1; el código actual ya lo descartaba — no es cambio mío).
- **`.bat` de update probado a mano**: camino de fallo escribe el marcador tras 10 tries; camino feliz mueve el exe y borra el marcador rancio; CRLF + `errorlevel` correctos.
- Diff de la fase: 13 archivos, +633 / −390. **NO se hizo `git push`** (esperando OK del usuario).

## En curso
- Nada. Fase 2 implementada, con review aplicado y verificada por máquina.

## Fase 2 — verificación FÍSICA: PENDIENTE del usuario (NO VERIFICADO)
La parte de datos reales y la visual **no se probaron en vivo** (necesitan datos reales / 2 instancias con `MILLENNIUM_INSTANCE` o las 2 PCs). A probar:
- **Datos:** agregar un favorito → matar el proceso a mitad → reabrir (debe seguir); corromper un JSON a mano (`{` suelto) → reabrir (favoritos a default PERO aparece `<archivo>.json.corrupt` + línea `ERR [jsonstore] parse failed...`).
- **UI:** `TARGET LOST` (cerrar el peer seleccionado, no debe re-apuntar); un `ERR transmit` que no se pisa a los 5 s; recibir texto y mandar otro (ACK) sin perder el entrante; barras TX/RX independientes; rename que sobrevive un `peers-changed`.

## Próximo paso CONCRETO
**Arrancar la Fase 3 de Windows (seguridad)** (`docs/remediation/windows/phase-3-security.md`) en un chat nuevo con `/inicio`: pinning real de certificado TLS + CSP + escaping de strings de peers + gate de `/text` + verificación del updater. Sumar el *unquoted path* del autostart (CWE-428, ya en TODO). La verificación física de la Fase 2 (arriba) puede hacerla el usuario cuando quiera; no bloquea la Fase 3 (independiente).

## Bloqueos
- **Android**: decisión estratégica previa pendiente (núcleo headless vs foreground-only, `docs/remediation/android/SPEC.md`). No arrancar Android sin decidirla.

## Pendiente derivado (no urgente, en TODO)
- **Zombie-killer mata una instancia SANA en doble-launch** (pre-existente, NO regresión de Fase 2 — confirmado por el review). Fix correcto: chequear liveness (probe `/info`) antes de matar. Más grande, no urgente.
- **Fragilidad del harness de test en Windows** (ver Contexto).
- **Autostart sin comillas** (va a Fase 3 seguridad).

## Archivos tocados esta sesión
- Backend: `json_store.rs` (nuevo), `preferences.rs`, `settings.rs`, `aliases.rs`, `icon_overrides.rs`, `manual_peers.rs`, `clipboard_sync.rs`, `windows_integration.rs`, `updater.rs`, `lib.rs`.
- Frontend: `main.js`, `index.html`, `styles.css`.

## Contexto que no está en otro doc
- **Divergencias / hallazgos con el spec de Fase 2:**
  - `settings.rs` no deriva `Default` → usa `JsonStore::load_with_default` (opción A del spec). `loaded_from_corrupt()` preservado (lo consume el heal de autostart).
  - `2.2.e` se pudo hacer COMPLETO (keyeo por `sessionId`) **sin tocar el backend**: los structs de evento ya tienen `#[serde(rename_all="camelCase")]`, así que `session_id` viaja como `sessionId`. El spec preveía tener que hacer el mínimo si no estaba.
  - El binario release se llama `millennium-clipboard.exe` (build) y para releases se **renombra a mano** a `Millennium Clipboard.exe` (con espacio) — por eso el zombie-killer matchea **ambos** nombres. La app que corre a diario es la del escritorio (`OneDrive\Desktop eQ\Millennium Clipboard.exe`, con espacio).
- **Harness de test de Tauri en Windows (importante para futuras fases con tests):** agregar CUALQUIER test al crate hace que el linker MSVC deje de podar el stack GUI de tao/wry en el binario de test del lib; ese binario importa símbolos comctl32-v6 (`TaskDialogIndirect`) sin el manifest que embebe `tauri-build` en el `.exe` real → `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139) al cargar, ANTES de correr ningún test. Diagnosticado con `llvm-readobj --coff-imports` (diff de imports: los tests suman ~145 símbolos Win32 GUI). Por eso los tests de `json_store` van `#[cfg(all(test, not(windows)))]` y la lógica se verifica en el harness aislado `scratchpad/jsonstore_verify` (no commiteado; es scratch). `cargo test --lib` en Windows queda VERDE (7/7 pre-existentes) porque los tests gateados no compilan.
- **Entorno**: PowerShell 5.1 rompe los `git commit -m` con comillas dobles; usar `git commit -F -` con heredoc desde el Bash tool.
