# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-20 · **Último commit**: `2955104` · **Branch**: `feat/displays` (**pusheado**, origin coincide) · **Working tree**: limpio.

> Este handoff cubre **DOS sesiones** del mismo día: la Fase 1 de displays, y el gate de Android que vino después. La segunda no llegó a correr su `/cierre`, así que sus docs se escribieron desde la sesión siguiente, **verificando cada claim contra los runs reales del CI** (no contra su reporte).

## Sesión 2 (última) — **Gate de Android en el CI: HECHO y PROBADO EN FALSO**

Cierra el hueco que la Fase 1 dejó declarado. `.github/workflows/android-cfg-gate.yml` (nuevo, archivo aparte — **no se tocó `build.yml`**): corre en `ubuntu-latest` y hace **una sola cosa**, `cargo check --target aarch64-linux-android`. No construye la app: solo type-checkea, que es todo lo que hace falta para cazar una **fuga de `cfg`**.

- **Evidencia verificada contra la API de GitHub** (no contra el reporte de la sesión):
  - `Android cfg gate` @ `2955104` (código sano) → ✅ **success**, 2,1 min.
  - `Android cfg gate` @ `488b4c4` (rama descartable, **con una fuga plantada a propósito**: se le sacó el `#[cfg]` a `views_from_topology`) → ❌ **failure**. **El gate está probado en falso**: se pone rojo cuando el código está mal. Sin esto sería decoración.
  - `Build Windows` @ `2955104` → ✅ **success**: no se rompió nada.
- **Costo real**: 2,1 min y **el NDK NO se descarga** — `ubuntu-latest` lo trae preinstalado en `ANDROID_NDK_ROOT`. Se necesita solo porque `ring` compila C en su `build.rs`. Nunca se acercó al timebox de 15 min, y corre **en paralelo** al job de Windows ⇒ el CI no tarda más en pared.
- **Se descartó `tauri android build --debug`**: 20-40 min, y depende de `src-tauri/gen/android/`, que es zona de la regla dura "NUNCA correr `tauri android init`". No agrega señal para este propósito: una fuga de `cfg` revienta en el type-check, mucho antes del linker o de Gradle.
- **Un solo ABI alcanza**: el `cfg` que decide es `target_os`, que vale igual para los cuatro. Chequear los otros tres sería pagar 4× por la misma respuesta.
- **Limitación honesta**: este gate caza fugas de `cfg` en **Rust**. NO cubre regresiones de Gradle/manifest/Kotlin — eso sigue necesitando un build de Android de verdad, a mano.
- La rama de prueba con la fuga se borró de local y de GitHub; nunca tocó `feat/displays`.

## Sesión 1 — SPEC-displays: **Fase 1 IMPLEMENTADA y VERIFICADA EN HARDWARE REAL**

Se migró de Monarch el camino de **lectura** del motor CCD y se expuso en un modal propio. **Cerró también el pendiente físico de la Fase 0** (correr el binario en la máquina de 3 displays), así que no queda nada colgado de la etapa anterior.

- **Vendor** (`src-tauri/vendor/monarch/`, 10 archivos): copia del crate puro desde **`guidocameraeq/Monarch` — el fork de Guido, NO el upstream de Nuzair46** — commit `7f9f63b`. LICENSE MIT íntegro con sus dos copyrights. Path-dep bajo el target-table de Windows ⇒ Android no lo ve.
- **Motor** (`src-tauri/src/displays/`): `enumerate.rs` + `win32_types.rs`, windows-only con doble gate. Solo ejecuta `GetDisplayConfigBufferSizes`, `QueryDisplayConfig` y `DisplayConfigGetDeviceInfo`.
- **Comando** `displays_get_snapshot`: async + `spawn_blocking`, **sin `cfg` en el `generate_handler!`** (decide el cuerpo; fuera de Windows devuelve `Err`) — patrón de `apply_update`.
- **Frontend**: botón HUD `DISP` + modal + lista con badges PRIMARY/ACTIVE/DETACHED. Render por diff, molde de `buildPeerItem`.
- **Reemplazó** el `mod ccd_link_smoke` de la Fase 0 (canario de linkeo, ya no aporta: el motor real llama la misma familia de funciones).
- **`docs/DECISIONS.md` nuevo** (6 ADRs + la doctrina CCD heredada para la Fase 2 + la nota de verificación).

## 🔑 Las 3 decisiones que se apartaron del SPEC (todas documentadas como ADR)

1. **NO se copió `topology.rs` ni `apply.rs`** (ADR-002). El SPEC pedía `enumerate/topology/win32_types/mod`, pero `topology.rs` importa **12 símbolos de `apply.rs`** y su `new()` llama `capture_sdr_gamma_ramps` ⇒ copiarlo arrastraba el motor de apply entero, con sus 5 `SetDisplayConfig`. Y **no hacía falta**: todo su andamiaje (cache, merges, persistencia) existe para **re-adjuntar**, no para leer — sus propios comentarios lo dicen. Se fueron con él: el `assume_init` sobre bytes de disco, el acople a `%APPDATA%\Monarch\config.json` (¡el config real de Monarch del usuario!) y el único `eprintln!`.
2. **Vendor por copia, no `git subtree`** (ADR-001): el subtree traía 80 archivos para usar 8, y un `subtree split --prefix=src` no incluye el `Cargo.toml` de la raíz.
3. **Sin estado ⇒ no se tocó `AppState` ni `setup()`** (ADR-005). Consecuencia del punto 1. Sin cache no hay `Mutex`, así que la regla de "nunca sostener un lock a través de un `.await`" se cumple **por construcción**.

## Evidencia (verificado, no supuesto)

- **CI VERDE**: run [29754851028](https://github.com/guidocameraeq/Millennium-Clipboard/actions/runs/29754851028), **6,5 min**, los 12 pasos `success`, artefacto `.exe` **4,19 MB** (el `Upload` usa `if-no-files-found: error` ⇒ el binario existe).
- **Prueba física del usuario en el desktop de 3 displays** (2026-07-20): aparecen **los 3 monitores reales**, **incluida la desconectada** (el caso difícil — Windows no la lista por el camino normal). **Regresión OK**: copiar/pegar y envío de archivos siguieron funcionando igual.
- **Verificación local ANTES del CI** (ver abajo, el hallazgo del harness): `cargo check` verde en **las dos ramas de `cfg`** (0 errores, 0 warnings) · **4 tests** del módulo en verde (ejecutados) · **22 tests** del vendor en verde · `node --check` OK.
- **Cero `SetDisplayConfig`**: `grep -rn "SetDisplayConfig\|ChangeDisplaySettings" src-tauri/src src-tauri/vendor | grep -v "//"` → **0 líneas**.

## 🔑 Hallazgo que cambia cómo se trabaja de acá en adelante

**El build local NO estaba roto por el crate `windows`.** El que pide `dlltool.exe` es **`parking_lot_core`**, una dependencia transitiva de Tauri. `windows 0.60` usa `windows-link`/raw-dylib y **el toolchain gnu lo chequea sin problema**.

⇒ El módulo de displays **se puede verificar local** en un crate scratch con las mismas dependencias reales (`windows 0.60` + mismas features + path-dep real al vendor + serde) y un `mod runtime_log` de mentira. `cargo check` ✅ type-checkea las dos ramas de `cfg`; `cargo test` ❌ (linkear sí pide `dlltool`) ⇒ para correr tests de lógica pura, segunda variante sin la dep `windows`. **Usar esto en las Fases 2 y 3**: es mucho más barato que quemar una corrida de CI. Receta completa en `docs/DECISIONS.md`.

## Review adversarial (5 lentes, 23 agentes) — 7 hallazgos reales, todos corregidos antes del push

Los dos que importaban:
- **El botón `DISP` se veía igual en Android**: el atributo `hidden` del HTML se apoya en la regla `[hidden]{display:none}` del navegador, y **cualquier** declaración de `display` del autor le gana. `.hud-btn` declara `display:inline-flex` (y `html.is-mobile .hud-btn` encima usa `!important`). Fix: `.hud-btn[hidden] { display: none !important; }`. **El codebase ya había tropezado con esto** (`.backend-banner[hidden]`, `.qr-pane[hidden]`).
- **El techo de buffers era demasiado bajo** (1024/2048 → **65 536/131 072**): `QDC_ALL_PATHS` es **combinatorio** (una entrada por cada source×target de cada adaptador). Con placa integrada + dedicada + virtuales se pasa, y el `Err` se lo tragaban el `let ... else` del seeder y el `.ok()` del enriquecimiento ⇒ el modo de falla era **"la TV desconectada no aparece", en silencio**. También se cambió el descarte silencioso por un log en la línea `enum:`.

## Próximo paso CONCRETO

**Chat nuevo → Fase 2 del SPEC-displays** ("Apply con red de seguridad"). El prerequisito de Android **ya está cerrado**, así que se puede ir directo.

Traer `apply.rs` **y** la mitad de `topology.rs` que hoy no está (cache + merges + persistencia — es la maquinaria del re-attach). **Re-implementar las DOS piezas de seguridad**: el resume-listener (`invalidate_backend_cache()` al despertar) **y** el watchdog de auto-rollback (el manager del crate puro es **pasivo**: guarda el deadline pero NO dispara; sin la glue, un layout malo queda pegado y nadie revierte = el bug de la TV). Front: countdown Confirmar/Revertir. **Leer `docs/DECISIONS.md` → "Doctrina CCD heredada" ANTES de escribir una línea.** Al instanciar el manager, apuntar su store al APPDATA de **Millennium**: el default escribe sobre el `config.json` real de Monarch del usuario.

## Bloqueos / huecos conocidos

- **CPU en reposo**: NO se verificó explícitamente en Task Manager esta sesión. El diff no agrega poll ni timer (la enumeración corre solo al abrir el modal o apretar REFRESH), así que el riesgo es teórico — pero queda sin evidencia.
- Los **4 tests** de `displays/mod.rs` están gateados `#[cfg(all(test, not(windows)))]` ⇒ **no corren ni local ni en CI** (el harness de tests del crate se rompe en Windows, pendiente viejo del TODO). Se ejecutaron a mano en el crate scratch. No son decoración, pero hoy nadie los corre automáticamente.

## Archivos tocados

- **Sesión 2**: `.github/workflows/android-cfg-gate.yml` (nuevo) · `.github/workflows/build.yml` (comentario: ya no es el único gate).
- **Código nuevo**: `src-tauri/src/displays/{mod,enumerate,win32_types}.rs` · `src-tauri/vendor/monarch/` (10 archivos, incluido `PROVENANCE.md`).
- **Código modificado**: `src-tauri/Cargo.toml` (+path-dep) · `src-tauri/Cargo.lock` (**tenía un agujero de la Fase 0: no reflejaba el `windows 0.60`**) · `src-tauri/src/lib.rs` (+mod, +comando, −smoke) · `src/{index.html,main.js,styles.css}` · `.github/workflows/build.yml` (comentario).
- **Docs**: `docs/DECISIONS.md` (nuevo) + este HANDOFF, CHANGELOG, TODO, y la línea 3 de `docs/SPEC-displays.md`.

## Contexto importante para la próxima sesión

- **El donante es el fork de Guido** (`guidocameraeq/Monarch` @ `7f9f63b`), NO el upstream. Él lo remarcó explícitamente: el valor de la migración ES su fork, porque es el que peleó y ganó contra la TV. Receta de re-sincronización en `vendor/monarch/PROVENANCE.md`.
- **`MonarchDisplayManager::new()` NO es read-only**: sincroniza huellas y puede escribir el store. Con el `FileConfigStore` por default esa escritura va a **`%APPDATA%\Monarch\config.json` — el config real de Monarch del usuario**. La Fase 1 no lo instancia; **cuando la Fase 2 lo haga, apuntarlo al APPDATA de Millennium**.
- **Convivencia de versiones `windows`**: la 0.60 (nuestra, pineada) y la 0.61.x (transitiva de tauri/wry) conviven, pero son **tipos incompatibles** — nunca cruzar un handle de la API de Tauri a una función del crate 0.60 sin pasar por `isize`/raw.
- **`runtime_log` expone `err`, NO `error`**, y son funciones que toman `impl Into<String>`, no macros con formato inline.
- El **centinela `0x0`** de un monitor desconectado no es un bug: Windows no reporta modo para un panel apagado. La UI lo muestra como "—".
- **En el escritorio del usuario quedó la carpeta `Millennium DISPLAYS - prueba`** (instructivo + `.bat` para abrir con monitores de mentira vía `MONARCH_FORCE_MOCK_BACKEND`). Se puede borrar cuando quiera.
