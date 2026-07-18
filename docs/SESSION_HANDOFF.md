# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-18 · **Último commit**: `5585175` · **Branch**: `feat/displays` (**pusheado**, origin coincide) · **Working tree**: limpio. **Esta sesión SÍ tocó código** (backend, aditivo y gateado a Windows).

## Qué se hizo — SPEC-displays: **Fase 0 IMPLEMENTADA y VERIFICADA (CI verde)**

Arrancó el SPEC de migrar el motor de monitores de **Monarch → Millennium** (`docs/SPEC-displays.md`, commit `a7224b3`). Se ejecutó **solo la Fase 0** (el prerequisito bloqueante: montar el CI y probar que `windows 0.60` linkea). Nada de código de displays real todavía — eso es Fase 1.

- **CI nuevo** (`.github/workflows/build.yml`, commit `5585175`): portado del `build-personal.yml` de Monarch, adaptado a Millennium → **npm** (no yarn), **sin instalador** (`bundle.active=false`), target `x86_64-pc-windows-msvc`, artefacto `millennium-clipboard.exe`. El crt-static ya venía del `.cargo/config.toml` (commit `5ffdfca`). Trigger: push a `feat/displays` + `workflow_dispatch`. `timeout: 90` (la 1ra corrida en frío compila dos versiones del crate `windows`).
- **`windows = 0.60`** con 10 features CCD (enumerate/apply/topology) agregado **SOLO bajo `[target.'cfg(target_os = "windows")'.dependencies]`** → **Android NO lo ve** (el riesgo #1 del SPEC). Pin 0.60 (convive con el 0.61.x transitivo de tauri/wry — dos árboles paralelos).
- **Smoke de linkeo** (`lib.rs`, `mod ccd_link_smoke`): llama `GetDisplayConfigBufferSizes` (función CCD raw-dylib REAL) desde `run()`, loguea por `runtime_log::info`, **sin `unwrap`/`expect`** (respeta `panic=abort`). Aditivo y no-fatal. Un dep sin usar no emite el import raw-dylib → por eso se referencia de verdad. **La Fase 1 lo reemplaza** por el backend migrado de Monarch.
- **Review adversarial** (workflow, 4 lentes: ci-yaml / rust-compile / android-cfg-leak / link-proof) ANTES del push → **4× would-pass, 0 blockers**. Único nit (bajo): `timeout 60→90` para la corrida en frío. Aplicado.

## Estado
- **Branch `feat/displays`**, pusheado (`origin/feat/displays` = `5585175`). **Working tree limpio** — todo commiteado y en GitHub.
- **Núcleo de Millennium INTACTO**: clipboard, discovery mDNS, servidor HTTPS, transferencias, pinning. El diff es **aditivo + gateado** (una llamada one-shot al arranque, sin poll, sin timer → CPU en reposo no se toca).
- **Compilación real**: el gate es el **CI MSVC verde** (compila + LINKEA de verdad; más fuerte que un `cargo check` local). **El build local sigue sin andar** (falta MSVC/`dlltool` en la máquina de Guido — pre-existente, es la premisa del SPEC: el binario sale del CI, no local). Correr `cargo check` local acá fallaría por el toolchain, no por el código.

## Evidencia de la Fase 0 (run **VERDE**)
- Run: https://github.com/guidocameraeq/Millennium-Clipboard/actions/runs/29650684956 · **11,4 min** · todos los pasos `success`.
- Pasos clave: `Build portable .exe` ✅ (⇒ `windows 0.60` compiló y **linkeó** raw-dylib en MSVC) · `Upload portable .exe` ✅ (`if-no-files-found: error` ⇒ el `.exe` existe de verdad).
- Artefacto: `millennium-clipboard-5585175…` · **4,2 MB** · vivo.
- **Criterios de aceptación Fase 0**: workflow verde ✅ · `.exe` portable producido ✅ · `windows 0.60` en el grafo de deps + linkeado ✅ → **el riesgo [ALTO] del SPEC (que `windows 0.60` no linkee) queda RETIRADO.**

## Próximo paso CONCRETO
1. **(Usuario, cierra la Fase 0 del todo)** Bajar el `.exe` del run verde y correrlo en el **desktop de 3 displays** → en el log tiene que aparecer `[displays] Fase 0 link smoke: GetDisplayConfigBufferSizes status=0 paths=N modes=M` (adelanto de confianza: la API de monitores responde en tu máquina). De paso, confirmar que **clipboard/discovery/transferencias siguen igual** (criterio de regresión — solo se prueba corriéndolo).
2. **Chat nuevo → Fase 1 del SPEC-displays** ("Ver los monitores", read-only, CERO `SetDisplayConfig`): vendorizar el crate puro `monarch` por **git subtree** en `src-tauri/vendor/monarch/`; copiar `enumerate/topology/win32_types/mod` + el enum `SystemDisplayBackend` tras `#[cfg(windows)]`; **un** comando `displays_get_snapshot` async con `spawn_blocking` (el `std::Mutex` DENTRO del closure, nunca a través del `.await`); front: botón HUD `.desktop-only` + modal + lista. `MockBackend` (`MONARCH_FORCE_MOCK_BACKEND`) como fallback dev cross-platform. Criterio: en el desktop de 3 displays aparecen los 3 reales con nombre/resolución/Hz + badges; `cargo test` del vendor = 22 verdes sin MSVC.
3. **(Follow-up, NO bloquea Fase 1)** Sumar el **build de Android al CI** — `cargo check` corre en el host y **no** detecta una fuga de `cfg` que rompa Android; solo `tauri android build` la revela. Hoy la Fase 0 está gateada correcta (review lo confirmó), pero conviene el guard automático antes de sumar más código Win32.

## Bloqueos
- Ninguno para la Fase 1. El riesgo bloqueante del proyecto (link de `windows 0.60`) ya está despejado.

## Archivos tocados
- **Código**: `.github/workflows/build.yml` (nuevo), `src-tauri/Cargo.toml` (+`windows 0.60`), `src-tauri/src/lib.rs` (+`ccd_link_smoke`).
- **Docs**: `docs/SPEC-displays.md` (nuevo, ya commiteado `a7224b3`; status actualizado a Fase 0 done), este HANDOFF, CHANGELOG, TODO.

## Contexto importante (para la próxima sesión)
- **El smoke NO es código de displays** — es un canario de link (prueba que el crate `windows` enlaza en el runner). La Fase 1 lo borra y pone el backend real. No lo trates como una feature.
- **CI**: corre en push a `feat/displays` o `workflow_dispatch` manual. La 1ra corrida en frío tardó 11,4 min (holgado vs el timeout de 90).
- **Convivencia de versiones `windows`**: la 0.60 (mía, pineada) y la 0.61.x (transitiva de tauri/wry) conviven sin choque — confirmado por el review y por el CI verde. **Mantener el pin 0.60** (la persistencia del snapshot en Fase 2/3 hace `memcpy` de structs `DISPLAYCONFIG_*`; un mismatch de layout invalidaría el snapshot).
- **Regla que mandó acá**: Monarch es **MIT** — al vendorizar en Fase 1 hay que **preservar el aviso de copyright** en el vendor y documentarlo en `docs/DECISIONS.md` (que todavía no existe en Millennium; lo crea la Fase 1).
- **Doctrina CCD intocable (para Fase 2)**: dry-run `SDC_VALIDATE` obligatorio, pre-estado como precondición dura, verificar **re-enumerando** (nunca por el status de retorno), y el **watchdog de auto-rollback** (manager pasivo + glue que dispara) — PROHIBIDO simplificarlo (es el bug de la TV irrecuperable que Monarch nació para matar).
