# SPEC: Módulo de Displays en Millennium — migración del motor de Monarch
Sumar a Millennium una sección de manejo de monitores (ver, attach/detach de la Smart TV con red de seguridad, perfiles y acomodar), trayendo el motor CCD ya probado de Monarch como módulo Windows-only. Millennium es el host que crece; Monarch es el donante.
- Estado: EN CURSO — **Fase 0 ✅** (CI verde, commit `5585175`) · **Fase 1 ✅ IMPLEMENTADA Y VERIFICADA EN HARDWARE** (commit `76afb8a`, run 29754851028, 3 monitores reales + la detached, regresión OK); **Fases 2-3 pendientes**. ⚠️ La Fase 1 se apartó del plan en 3 puntos, todos justificados en `docs/DECISIONS.md`: **`topology.rs` y `apply.rs` NO se copiaron** (ADR-002 — la Fase 2 los trae), vendor por copia y no `git subtree` (ADR-001), y sin campo en `AppState` (ADR-005).
- Fecha: 2026-07-18
- Origen: destilado de la súper-investigación en `../docs/INVESTIGACION-migracion-displays.md` del hub (15 agentes, evidencia file:line, verificación adversarial). Formato: SPEC delta (feature sobre app existente).

## Por qué (el dolor)
Guido usa DOS apps separadas: Monarch (gestor de monitores, fork que arregló) y Millennium (su clipboard/transferencia LAN). Quiere dejar de depender de dos programas y tener el manejo de monitores DENTRO de Millennium, su app propia. El motor de Monarch es valioso porque peleó y ganó contra la CCD API de Windows (attach/detach de una Smart TV HDMI sin panel de rescate). En vez de reescribir ese motor, se **migra** a Millennium.

## Contexto del código (nombres REALES, de la investigación)

### Host: Millennium (`millennium-clipboard/`)
- Tauri 2, backend Rust (~5.5k LOC en `src-tauri/src/`) + frontend **JS/CSS vanilla sin framework ni bundler** (`src/`).
- Empaqueta **portable** (`src-tauri/tauri.conf.json`: `bundle.active=false` → artefacto `src-tauri/target/release/millennium-clipboard.exe`). El objetivo del `.exe` portable ya está resuelto host-side.
- Molde de módulo Windows-only YA existente: `src-tauri/src/windows_integration.rs`, y el bloque `[target.'cfg(windows)'.dependencies]` de `src-tauri/Cargo.toml` (~línea 78).
- Registro de comandos: **un solo** `generate_handler!` en `src-tauri/src/lib.rs` (~línea 1622). Estado compartido: `AppState` (`lib.rs` ~línea 62), instanciado en `setup()` (`lib.rs` ~línea 1208).
- Frontend: molde de render por diff `renderPeers`/`buildPeerItem`/`updatePeerItem` con `data-id` + `textContent` (`src/main.js` ~484-652); molde de modal tipo LOG (`src/index.html` ~461); loop `listen('...changed') → state → render` (`main.js` ~2028). Clase `.desktop-only` que ya se oculta en mobile (`src/styles.css` ~2151).
- Persistencia atómica: `JsonStore` (tmp+rename, backup-on-corrupt). Logging: `runtime_log::info/warn/error` (NUNCA `println!`).
- `[profile.release]` con `panic=abort` (`Cargo.toml` ~99-104).
- **Está en remediación por fases** (`docs/remediation/`): 3 fases Android (A/B/C) son specs intocables, bloqueadas esperando la decisión "núcleo headless vs foreground-only" (`docs/TODO.md`).

### Donante: Monarch (fork MIT de Nuzair46/Monarch)
- `Monarch/src/` — **crate puro `monarch`**: lógica de perfiles/layout/confirmación + `MockBackend` + **22 tests**. Deps: solo `serde`/`serde_json`. Cero Windows, cero Tauri.
- `Monarch/src-tauri/src/backend/windows/` — **el motor CCD real** (enumerate/apply/topology/win32_types/mod). **Verificado: isla sin Tauri** (cero `tauri::`, cero `AppHandle`, cero `emit`). Único acople: `crate::diagnostics` (logger de 66 líneas).
- `Monarch/src-tauri/src/app/` — glue de Monarch (tray, IPC, single-instance, shortcuts, startup). **NO viaja** (Millennium tiene lo suyo), PERO contiene dos piezas de seguridad que SÍ hay que re-implementar (ver Riesgos): el resume-listener y el **watchdog de auto-rollback** (`app/events.rs` ~42-77, gatillado tras cada apply en `commands.rs` ~173/212/266).
- Doctrina CCD (cicatrices) que viven DENTRO de `apply.rs`/`topology.rs` y viajan solas: dry-run `SDC_VALIDATE` obligatorio, pre-estado como precondición dura, verificar re-enumerando (nunca por el status de retorno). Documentadas en `Monarch/docs/DECISIONS.md` (ADR-003/004/008/009).
- Dep clave del motor: `windows = 0.60` (raw-dylib). Licencia: **MIT** (`Monarch/LICENSE`).

## AGREGA (lo nuevo)
1. **`src-tauri/src/displays/`** — módulo nuevo, TODO tras `#[cfg(target_os="windows")]`, declarado en `lib.rs` junto a `windows_integration`. Contiene el backend CCD **copiado y adaptado** de Monarch (imports `monarch::` → path del vendor; `crate::diagnostics` → shim a `runtime_log`).
2. **`src-tauri/vendor/monarch/`** — el crate puro `monarch` vendorizado por **git subtree** (con Monarch agregado como remote), consumido como path-dep bajo el bloque `[target.'cfg(windows)'.dependencies]`. Sus 22 tests corren local sin MSVC (`cd src-tauri/vendor/monarch && cargo test`).
3. **`windows = 0.60`** (+ features CCD) en `Cargo.toml`, SOLO bajo el target-table windows.
4. **Comandos Tauri nuevos**, gateados `#[cfg(target_os="windows")]` en el `generate_handler!` único: `displays_get_snapshot`, `displays_toggle`, `displays_apply_layout`, `displays_confirm`, `displays_save_profile`, `displays_load_profile` (nombres finales a criterio del constructor). Async con `spawn_blocking`.
5. **Campo `#[cfg(windows)]` en `AppState`** para el estado de displays; init **no-fatal** en `setup()`.
6. **Frontend**: botón HUD `data-action="displays"` (`.desktop-only`) + `#displays-modal` que espeja el molde del modal LOG + vista de lista/perfiles/lienzo, siguiendo el molde de render por diff. Evento `displays-changed` → state → render.
7. **CI nuevo** (`.github/workflows/…`): portada del `build-personal.yml` de Monarch adaptada a Millennium (sin yarn/vite, sin `--bundles msi`; `frontendDist ../src`, `bundle.active=false`, target `x86_64-pc-windows-msvc` + crt-static, artefacto `millennium-clipboard.exe`).
8. **`docs/DECISIONS.md`** de Millennium: nuevo, documentando qué código vino de Monarch, de qué commit, y la doctrina CCD (copiar ADR-003/004/008/009). Preservar el aviso MIT de Monarch.
9. **`ConfigStore` de displays** sobre el `JsonStore` atómico de Millennium, repunteado a `%APPDATA%\Millennium` (reemplaza el `FileConfigStore` no-atómico del motor donante).

## MODIFICA (lo existente que se toca — con su efecto colateral a cuidar)
- **`src-tauri/src/lib.rs`** `generate_handler!` (~1622): agregar los comandos nuevos con entradas `#[cfg(target_os="windows")]`. → El resto de comandos existentes debe seguir registrándose y funcionando igual.
- **`AppState`** (~62) + **`setup()`** (~1208): campo nuevo gateado, init no-fatal. → El arranque en no-Windows y en Android debe seguir igual (si displays no inicia, la app arranca lo mismo).
- **`src/index.html` / `src/main.js` / `src/styles.css`**: botón HUD + modal + vista nuevos. → El HUD actual y el render de peers existente NO cambian de comportamiento.
- **`src-tauri/Cargo.toml`**: `windows 0.60` + path-dep del vendor SOLO bajo `[target.'cfg(windows)'.dependencies]`. → El build de **Android NO debe ver `windows 0.60`** (si lo ve, rompe).

## NO SE TOCA (el seguro anti-regresión — criterio #1 del smoke)
- **El núcleo de Millennium**: clipboard, discovery mDNS, servidor HTTP/axum, transferencias, pinning de certificados. Displays es greenfield (grep confirmó cero código de monitores/CCD hoy, cero colisión de nombres/comandos/eventos).
- **El protocolo de peers**: no romper compat con peers viejos. Los DTOs de displays son nuevos y viven detrás de comandos nuevos; NO tocan el hello UDP ni el JSON de `/info`.
- **`docs/remediation/`** (las 3 fases Android): se ejecuta, NO se reescribe. Displays entra SIN editar esos `.md`.
- **`[profile.release]` `panic=abort` y la topología de crate**: se queda como está. **NADA de workspace** (movería el profile y agrandaría la superficie `cfg` hacia Android).
- **La persistencia atómica**: usar `JsonStore` (tmp+rename), NUNCA `fs::write` a mano (reintroduce bugs de corrupción que Millennium ya arregló).
- **El CPU en reposo ~0%**: nada de poll activo; watcher por evento (`WM_DISPLAYCHANGE`) en un thread dedicado que solo despierta ante cambio real.

## Plan por fases (orden obligatorio; cada fase se aprueba y prueba antes de la siguiente)

### Fase 0 — CI verde (PREREQUISITO BLOQUEANTE, antes de una línea de displays)
Crear el workflow de CI portado de Monarch. En el mismo PR: agregar `windows = 0.60` y el path-dep del vendor aunque todavía no los use nadie, para probar que linkean en el runner.

### Fase 1 — Ver los monitores (read-only, CERO `SetDisplayConfig`)
Vendorizar el crate puro por subtree. Copiar `enumerate/topology/win32_types/mod` + el enum `SystemDisplayBackend` tras `#[cfg(windows)]`. UN comando `displays_get_snapshot` async con `spawn_blocking` (el `std::Mutex` tomado y soltado DENTRO del closure, nunca a través del `.await`). Front: botón HUD + modal + lista. `MockBackend` (`MONARCH_FORCE_MOCK_BACKEND`) como fallback dev cross-platform.

### Fase 2 — Apply con red de seguridad (attach/detach de la TV)
`displays_toggle`/`displays_apply_layout` que llaman `SetDisplayConfig`, cargando la doctrina CCD completa de `apply.rs`/`topology.rs`. **Re-implementar en la glue nueva las DOS piezas de seguridad** (ver Riesgos): resume-listener (`invalidate_backend_cache()` al despertar) Y watchdog de auto-rollback al vencer el timeout. Front: countdown Confirmar/Revertir + evento `displays-changed`.

### Fase 3 — Perfiles, ajustes y watcher en vivo
`ConfigStore` sobre `JsonStore` repunteado al APPDATA de Millennium. Tabs de perfiles y ajustes. Watcher por evento (`WM_DISPLAYCHANGE`). El **lienzo de arrastre (drag-to-arrange) es trabajo NET-NEW** (no existe en Monarch) — se construye acá.

## Criterios de aceptación (verificables, por fase, empezando por regresión)
- **REGRESIÓN (toda fase)**: clipboard, discovery y transferencias siguen andando igual; `tauri android build` sigue compilando; CPU en reposo ~0%. (Es el criterio #1 del `/smoke`.)
- **Fase 0**: el workflow corre **VERDE** en el runner y produce el `.exe` portable con `windows 0.60` en el grafo de deps. Evidencia: link al run verde + el `.exe` (no "quedó andando"). En paralelo: `cargo check` local sigue pasando; `tauri android build` sigue compilando.
- **Fase 1**: en el desktop de 3 displays (2 monitores + Smart TV), con el `.exe` del CI: click en DISPLAYS → aparecen los 3 monitores reales con nombre/resolución/Hz + badges Active/Detached/Primary. `cargo test` en el vendor = **22 verdes sin MSVC**. Cero `SetDisplayConfig` ejecutado.
- **Fase 2**: attach/detach de la TV funciona; el countdown aparece y el **auto-rollback dispara solo** si no confirmás; tras cada operación se verifica **re-enumerando** y NUNCA queda un display perdido. Auditoría: **0 `unwrap`/`expect`** en el camino apply/enumerate/topology (con `panic=abort`, un panic tumba TODO Millennium). Smoke obligatorio en Windows 11 24H2 (re-sondar con `SDC_VALIDATE`). Evidencia: `diagnostics.log` + reproducción E2E.
- **Fase 3**: perfiles guardan/cargan sobre `JsonStore` (atómico); el watcher por evento refleja cambios de monitor en vivo sin poll; el lienzo de arrastre acomoda y persiste. Los datos de perfiles del usuario NUNCA se pierden ni migran sin su OK.

## Supuestos
- [ALTO] El runner de CI logra linkear `windows 0.60` (usa raw-dylib). Si NO, todo el plan traba en Fase 0 y hay que buscar alternativa de linkeo. Es lo primero a despejar.
- [ALTO] La direccón es Monarch→Millennium únicamente (Millennium es el host que queda). Confirmado por Guido.
- [BAJO] UI = botón HUD propio + modal (no sub-sección de Settings). Default elegido; barato de cambiar.
- [BAJO] La config de displays vive en el store de Millennium (`%APPDATA%\Millennium`). Default elegido.

## Riesgos y decisiones ⚠️
- ⚠️ **Watchdog de auto-rollback (el riesgo de mayor consecuencia)**: la red de auto-revert necesita DOS piezas — el **manager** del crate puro (la política: guarda el deadline) **+** la **glue** nueva (el gatillo: un watchdog spawneado tras cada apply que llama al rollback al vencer el timeout). El manager es **pasivo**: si la glue no dispara, el layout malo queda pegado y nadie revierte → exactamente el bug de la TV irrecuperable. **PROHIBIDO simplificar esto.** Consecuencia de omitirlo: se reintroduce el bug que Monarch nació para matar.
- ⚠️ **Build local imposible / CI inexistente**: `windows 0.60` no linkea sin MSVC (falta en la máquina de Guido) y Millennium hoy no tiene CI. Consecuencia: sin Fase 0 verde, nada del resto se puede probar. Por eso es bloqueante.
- ⚠️ **`panic=abort` en release**: un panic en el camino CCD tumba todo el proceso de Millennium. Consecuencia: 0 `unwrap`/`expect` en apply/enumerate/topology (criterio de Fase 2).
- ⚠️ **Fuga de `cfg` que rompe Android**: `windows 0.60` + el código Win32 deben quedar TODO bajo `cfg(windows)`. `cargo check` corre en el host y NO detecta un `cfg` olvidado — SOLO `tauri android build` lo revela. Consecuencia: correr el build Android en CI, no confiar en `cargo check`.
- ⚠️ **Alineación de versión del crate `windows`**: la persistencia del snapshot hace `memcpy` de structs `DISPLAYCONFIG_*`; un mismatch de layout entre versiones invalida el `topology_snapshot.json`. Hay guard por `size_of` (rechaza el archivo, no corrompe silencioso). Mantener el pin `0.60`. (Verificado: `0.60` y el `0.61.3` transitivo de tauri/wry conviven sin choque — dos árboles paralelos.)
- ⚠️ **Colisión con la remediación Android en curso**: los dos únicos puntos compartidos inevitables (`generate_handler!` y `AppState`/`setup`) se tocan con cuidado quirúrgico. Recomendación: landear displays **mientras Android está pausado** (lo está), para no pelear merges sobre el mismo `run()`/`setup`.
- ⚠️ **Licencia**: Monarch es MIT (Nuzair46 + Guido Camera). Absorber está permitido; **preservar el aviso de copyright** en el vendor y en `docs/DECISIONS.md`.

## Decisiones abiertas menores (el constructor puede tomarlas con estos defaults)
- Nombres finales de comandos/eventos: a criterio del constructor, siguiendo las convenciones de Millennium.
- El lienzo de arrastre (Fase 3) es lo más caro y net-new: se puede arrancar con lista + toggles + Attach/Detach (cubre el 90%) y dejar el arrastre para el final o descartarlo si el tiempo aprieta. Guido pidió scope completo, así que entra, pero es el candidato natural a recortar si hace falta.
