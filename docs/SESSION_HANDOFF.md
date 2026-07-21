# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-20 · **Branch**: `feat/displays` · **Working tree**: limpio.

## En una línea

**La Fase 2 del SPEC-displays está HECHA, verificada en hardware y RELEASEADA.** En el desktop de 3
displays: la TV se apaga y se prende, si no se confirma vuelve sola, si se confirma se queda, y los
otros dos monitores no se movieron. Se publicó como **v1.1.0** (prerelease) y **el auto-updater
1.0.0 → 1.1.0 se probó de punta a punta y funciona**. De yapa quedó `release.yml`: los próximos
releases salen con un solo `git tag`. **No queda nada pendiente de esta misión** — lo abierto son
cosas menores para otra sesión (ver TODO: pasar la 1.1.0 a release final, regresión de transferencia,
estrenar `release.yml`, y la Fase 3).

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
| **Auto-updater 1.0.0 → 1.1.0** (CHECK → DOWNLOAD & RESTART) | ✅ **verificado por Guido** |
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
hash coincide con el SHA-256 real del archivo (`a00be6ee…c179dc60`). El `.exe` reporta `1.1.0` en
sus metadatos.

`gh` quedó instalado (v2.96.0) y autenticado como `guidocameraeq`, así que los próximos releases se
publican desde acá sin trámite.

### ⚠️ Corrección — de dónde saca el hash el updater (esto estaba mal escrito antes)

`updater.rs:146-150`: **prefiere el campo `digest` que GitHub calcula solo por cada asset**
(`"sha256:<hex>"`), y **solo si no está** se cae a buscar un token de 64 hex en el cuerpo del
release. El comentario del código lo dice con todas las letras: el enfoque solo-cuerpo *"abortaba en
todos los releases actuales"*, y se arregló el 2026-07-13.

Consecuencias, verificadas contra la API:
- **Todos** los releases del repo tienen `digest` en su asset, incluido `v1.0.0`. O sea que poner el
  hash en el cuerpo es **cinturón extra, no requisito**. (Vale igual: cuesta cero y cubre un asset
  sin digest.)
- Queda **descartada** la sospecha de que el camino de actualización viniera roto por falta de hash.
  Lo que decida si funciona es la prueba de abajo, no ese detalle.

### 🐛 Bug destapado por publicar como prerelease: la landing sirve la versión vieja

`GET /releases/latest` devuelve **`v1.0.0`**, no la 1.1.0: GitHub **excluye los prereleases** de
"latest". El botón de descarga del README y de la landing apuntan ahí, así que **quien entre de cero
se baja la 1.0.0**. Al updater no lo afecta (usa la lista completa, donde los prereleases sí
cuentan), así que esto golpea solo a usuarios nuevos. Se arregla cuando la 1.1.0 pase de prerelease
a release final (`gh release edit v1.1.0 --prerelease=false`), que era el plan una vez que se use
unos días sin sorpresas.

### Releases de acá en adelante: automáticos por tag

`.github/workflows/release.yml` (nuevo) publica el release solo. El flujo completo es:
1. Subir la versión en `src-tauri/Cargo.toml`, `tauri.conf.json` y `Cargo.lock` (los tres).
2. Commit + push a la rama.
3. `git tag v1.2.0 && git push origin v1.2.0`.

El workflow compila el `.exe`, **verifica que el tag coincida con la versión del código** (si no,
falla antes de compilar), publica con el asset adjunto y **relee el release como lo ve el updater**.
Tag limpio (`v1.2.0`) → release final; con sufijo (`v1.2.0-beta.1`) → prerelease. **Aún no tuvo su
primera corrida real** — el guard está probado en falso local (pasa con el tag correcto, falla con
uno adelantado/viejo/lock desfasado), y los pasos de build son copia verbatim de `build.yml` (verde),
pero el publish end-to-end se ejerce recién en la próxima versión.

### Auto-updater 1.0.0 → 1.1.0: ✅ PROBADO por Guido

CHECK FOR UPDATE ofreció la 1.1.0 y DOWNLOAD & RESTART dejó la app en la versión nueva. El camino de
actualización de punta a punta **funciona** — era lo último que faltaba de la 1.1.0, ya no queda
nada pendiente de esa versión.

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
