# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-20 · **Branch**: `feat/displays` · **Working tree**: limpio.

## En una línea

**La Fase 2 del SPEC-displays está HECHA y verificada en el hardware real.** En el desktop de 3
displays: la TV se apaga y se prende, **si no se confirma vuelve sola**, si se confirma se queda, y
los otros dos monitores no se movieron. Los 3 workflows del CI en verde. Lo que sigue es publicar
`v1.1.0` como prerelease para que el auto-updater la levante.

## Lo que se hizo

- **Motor de apply portado de Monarch** (`apply.rs`, 934 líneas): las 5 llamadas a `SetDisplayConfig`
  con sus combinaciones exactas de flags, la sonda `SDC_VALIDATE` obligatoria, y el guardado/
  restauración de gamma, calibración de color y fondos alrededor del cambio. Un verificador comparó
  función por función contra el donante: cero drift.
- **Backend con la escalera de rescate** (`topology.rs`, ~1300 líneas): cache en `Mutex`, merges,
  remapeo de `DisplayId`, y los 4 escalones — attach explícito (batch creciendo de a uno, cada paso
  sondeado) → `SDC_TOPOLOGY_EXTEND` → `DisplaySwitch /extend` → rollback + error preciso.
- **El watchdog de auto-rollback** (`watchdog.rs`), que es **la pieza que el crate puro no tiene**.
  Escrito, no copiado: el del donante tenía una carrera que podía dejar el layout pegado para
  siempre (ADR-009). 8 tests.
- **El resume-listener** (`system_events.rs`): ventana oculta que tira el cache al despertar la
  máquina. Cero CPU en reposo (`GetMessageW` bloquea, no hay poll).
- **El store apuntado al APPDATA de Millennium** (`store.rs`), sobre el `JsonStore` atómico, con un
  test que se pone rojo si la ruta llegara a tocar `%APPDATA%\Monarch`.
- **Frontend**: ATTACH/DETACH por fila + barra de cuenta regresiva con CONFIRMAR / REVERTIR AHORA,
  que se rehidrata si cerrás y reabrís el modal.
- **Glue de Tauri**: arranque **no-fatal** (si el motor no levanta, Millennium anda igual) y los
  comandos toman el estado con `try_state`, nunca con `State<...>` — Tauri **panica** al resolver un
  State ausente, y con `panic = "abort"` eso mata también el portapapeles.

## Tres cosas que aparecieron y valen más que el port

1. **Los tests de este repo no corrían en NINGÚN lado.** Ni local (falta linker para el target
   no-Windows) ni en CI (`build.yml` nunca invocó `cargo test`). Eran verdes por no existir. Se
   armó `src-tauri/displays-tests/` + su workflow: 13 tests de la red de seguridad en cada push, con
   un paso que falla si corrieron menos de los esperados. **ADR-011.**
2. **Un segundo camino escribía en `%APPDATA%\Monarch`** además del store señalado: la persistencia
   binaria del snapshot de topología. Y era el mismo código que hacía `assume_init` sobre bytes
   leídos de disco. No se portó. **ADR-008.**
3. **Dos agujeros en la propia red**, encontrados trazando el camino del apply: si la
   re-enumeración de verificación fallaba, o si el rollback inmediato fallaba, el cambio quedaba
   aplicado y **sin nadie persiguiéndolo** — el bug exacto que la fase viene a matar. Los dos ahora
   dejan el watchdog armado. La regla quedó escrita en el código: pasado el punto de aplicar, ningún
   camino puede salir sin watchdog, y por eso ahí no hay un solo `?`.

## Estado real, sin maquillaje

| Qué | Estado |
|---|---|
| `cargo check` rama Windows (crate scratch, archivos reales por `#[path]`) | ✅ sin advertencias |
| `cargo check` rama no-Windows (caza fugas de `cfg`) | ✅ sin advertencias |
| `cargo test` en `src-tauri/displays-tests` | ✅ **13 passed** |
| `cargo test` en `vendor/monarch` | ✅ **22 passed** |
| `node --check src/main.js` | ✅ |
| CI @ `9534822` — Android cfg gate (0,7 min) · Displays logic tests (1,2 min) · Build Windows (6,3 min) | ✅ **los 3 verdes** |
| **Hardware — la TV se apaga (DETACH) y se prende (ATTACH)** | ✅ **verificado por Guido** |
| **Hardware — no confirmar ⇒ vuelve sola** (EL criterio de la fase) | ✅ **verificado por Guido** |
| **Hardware — confirmar ⇒ se queda** | ✅ **verificado por Guido** |
| **Hardware — los otros 2 monitores no se movieron** | ✅ **verificado por Guido** |
| **Regresión: clipboard / discovery / transferencias** | ⬜ **NO PROBADO** — la app corrió, pero no se ejerció una transferencia |
| **CPU en reposo** | ⬜ **NO MEDIDO** (el diseño no agrega poll, pero eso es argumento, no evidencia) |

**Review adversarial automático: NO CORRIÓ** — los 6 agentes murieron por límite de sesión. Se hizo
una auditoría a mano que cubrió: cero `unwrap`/`expect`/`panic!` fuera de tests (verificado línea por
línea), todos los `.lock()` manejan el error, el timer del frontend se limpia en todos los caminos de
salida, y el único `innerHTML` es un template estático sin datos del backend. **Lo que quedó sin
auditar a fondo**: deadlocks entre el watchdog y un comando concurrente (el orden de toma de locks es
siempre manager → cache, revisado a mano, pero no exhaustivamente), y la rama Android de la glue
nueva de `lib.rs` — esa la cubre el gate de Android del CI.

## Próximo paso CONCRETO — publicar `v1.1.0` como prerelease

La versión ya está subida a **1.1.0** en `Cargo.toml`, `tauri.conf.json` y `Cargo.lock` (el CI corre
con `--locked`, así que el lock **tiene** que ir en el mismo commit).

Cómo funciona el auto-updater de esta app (verificado leyendo `src-tauri/src/updater.rs`):
- Consulta `GET /repos/guidocameraeq/Millennium-Clipboard/releases?per_page=30`, descarta los
  **borradores** y se queda con el primero. **Los prereleases SÍ cuentan** — es el modo normal acá.
- Compara el `tag_name` contra la versión compilada. Tag esperado: **`v1.1.0`**.
- Baja el asset `.exe` y **verifica su SHA-256 contra un token de 64 caracteres hexadecimales que
  busca en el CUERPO del release**. Si el cuerpo no tiene ninguno, **se niega a instalar** (es
  fail-safe a propósito). Y agarra **el primero** que encuentra, así que no puede haber otro hex de
  64 antes que el bueno.

**HECHO** — prerelease `v1.1.0` publicado sobre `ee406cf`:
https://github.com/guidocameraeq/Millennium-Clipboard/releases/tag/v1.1.0

Verificado **contra la API pública, simulando lo que hace el updater** (no contra la web): se queda
con `v1.1.0` (prerelease, no borrador), encuentra el asset `millennium-clipboard.exe` (9,9 MB), y el
token de 64 hex que lee del cuerpo **coincide** con el SHA-256 real del archivo
(`a00be6ee…c179dc60`). El `.exe` reporta `1.1.0` en sus metadatos.

`gh` quedó instalado (v2.96.0) y autenticado como `guidocameraeq`, así que los próximos releases se
publican desde acá sin trámite.

### Lo único que queda: probar el updater de verdad

Abrir una copia de la **v1.0.0** → Settings → APP UPDATES → CHECK FOR UPDATE → DOWNLOAD & RESTART, y
confirmar que queda en 1.1.0. **Sin esa prueba, el release está publicado pero el camino de
actualización no tiene evidencia.**

Dato que hace esta prueba más interesante de lo normal: **el release v1.0.0 NO tiene un hash de 64
hex en su cuerpo**, y el updater aborta sin hash. O sea que el camino de actualización probablemente
viene roto desde antes, y esta es la primera versión publicada con el hash como corresponde. Si el
CHECK desde la 1.0.0 funciona, se arregló solo; si falla, mirar `updater.rs::extract_sha256` primero.

## Bloqueos

Ninguno.

## Archivos tocados

**Nuevos**: `src-tauri/src/displays/{apply,topology,backend,store,watchdog,system_events,ids}.rs` ·
`src-tauri/displays-tests/` · `.github/workflows/displays-tests.yml`
**Modificados**: `src-tauri/src/displays/{mod,enumerate,win32_types}.rs` · `src-tauri/src/lib.rs` ·
`src/{main.js,index.html,styles.css}` · `docs/{DECISIONS,CHANGELOG,TODO,SPEC-displays}.md`

## Contexto que no está en otros docs

- **El gate local mejoró y conviene usarlo en la Fase 3**: el crate scratch ya no copia archivos, los
  incluye con `#[path]`, así que chequea el código que de verdad va al binario. Y
  `cargo check --target x86_64-unknown-linux-gnu` da la misma respuesta que el workflow de Android
  en 2 segundos. La receta completa está al final de `docs/DECISIONS.md`.
- **`watchdog.rs`, `store.rs` e `ids.rs` no pueden empezar a usar `windows` ni `tauri`.** Si lo
  hacen, salen del crate de tests y la red de seguridad vuelve a quedar sin probar. Es una
  restricción de diseño, no una casualidad.
- El scratch de `cargo check` vive fuera del repo (en el temp de la sesión); si hace falta rearmarlo,
  la receta está en `DECISIONS.md`. El que **sí** está en el repo y es permanente es
  `src-tauri/displays-tests/`.
