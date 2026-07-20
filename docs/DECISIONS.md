# DECISIONS — Millennium Clipboard

> Por qué el proyecto es como es. Un ADR por decisión técnica que tuvo alternativas reales.
> Se agrega en `/cierre` cuando hubo una decisión con alternativas; no se reescribe la historia.

---

## ADR-001 — El crate `monarch` se vendoriza por **copia**, no por `git subtree`

**Decisión**: `src-tauri/vendor/monarch/` es una copia byte a byte de 8 archivos del repo
**`guidocameraeq/Monarch`** (el fork de Guido, NO el upstream `Nuzair46/Monarch`), commit
**`7f9f63ba59a022f296c94ac85ff0a41adfce0324`** (`7f9f63b`, 2026-07-16). La trazabilidad la da ese
commit anotado en `vendor/monarch/PROVENANCE.md`.

**Alternativa descartada — `git subtree add`** (que era lo que pedía el SPEC): el crate puro vive en
la **raíz** del repo Monarch (`Cargo.toml` arriba, fuentes en `src/`), mezclado con `src-tauri/`,
`web/`, `docs/`, `package.json` y los workflows de CI de Monarch. Un subtree del repo entero habría
traído **80 archivos para usar 8**. Y `git subtree split --prefix=src` deja afuera el `Cargo.toml`
de la raíz, así que ni siquiera produce un crate válido.

**Lo que se pierde**: el linaje automático de git. Re-sincronizar es a mano (receta en
`PROVENANCE.md`). Costo real bajo: solo **10 de los 51 commits** de Monarch tocaron `src/`.

**Verificado, no supuesto** — el riesgo que hacía peligrosa esta decisión era que un crate anidado
adentro de `src-tauri/` se volviera **miembro implícito de workspace**, lo que movería el
`[profile.release]` (y con él `panic = "abort"`) y agrandaría la superficie de `cfg` hacia Android.
No pasa: `cargo metadata` sobre el proyecto real devuelve `workspace_members = [millennium-clipboard]`
y nada más, y el dep resuelve como `monarch -> target cfg(target_os = "windows")`.

**Licencia**: Monarch es MIT. El `LICENSE` viaja íntegro en el vendor, con **sus dos** líneas de
copyright (Nuzair46 por el upstream + Guido Camera por el fork). No se borra ninguna de las dos.

---

## ADR-002 — La Fase 1 copia **solo el camino de lectura**: `apply.rs` y `topology.rs` no viajan

**Decisión**: `src-tauri/src/displays/` contiene únicamente `enumerate.rs` (podado) y
`win32_types.rs` (podado). **`apply.rs` (845 líneas) y `topology.rs` (1331) no existen en este
repo.** El motor de la Fase 1 no tiene estado: `displays::snapshot()` llama
`enumerate::query_active_topology()` y traduce el resultado al DTO del frontend.

**El problema**: el SPEC decía "copiar enumerate/topology/win32_types/mod". Pero `topology.rs`
importa **12 símbolos de `apply.rs`** en su encabezado, y su `new()` llama `capture_sdr_gamma_ramps`.
Copiado tal cual, la Fase 1 —que es *"mirar, no tocar"*— habría arrastrado el motor de apply entero,
con sus 5 `SetDisplayConfig` adentro.

**Por qué no hacía falta**: todo el andamiaje de `topology.rs` (el cache con `Mutex`, los merges, la
persistencia binaria del snapshot) existe **para poder re-adjuntar** un monitor. Sus propios
comentarios lo dicen: *"This keeps a recently-detached display path available for re-attach"*. Para
una foto read-only bajo demanda no aporta nada: `query_active_topology()` ya devuelve `displays` +
`layout`, y `seed_connected_inactive_displays` ya hace visibles los monitores desconectados.

**Lo que se ganó al no copiarlo**:
- `SetDisplayConfig` **no está en el binario**. "Cero apply" pasa de promesa a hecho verificable.
- No hay `std::sync::Mutex` en el módulo ⇒ la regla dura *"nunca sostener un lock a través de un
  `.await`"* se cumple **por construcción**: no hay nada que sostener.
- No hay estado ⇒ no se toca `AppState` ni `setup()` (ver ADR-004).
- Se fue la persistencia binaria, que hacía `assume_init` sobre bytes leídos de un archivo de disco
  validando solo el tamaño: un archivo corrupto producía structs `DISPLAYCONFIG_*` basura.
- Se fue el acople a `monarch::FileConfigStore::default_config_path()`, que apunta a
  **`%APPDATA%\Monarch\config.json` — el config real de Monarch del usuario**.
- Se fue el único `eprintln!` del motor (Millennium loguea por `runtime_log`, nunca `println!`).

**Lo que cuesta**: la Fase 2 tiene que traer `apply.rs` **y** la mitad de `topology.rs` que hoy no
está (cache + merges + persistencia). Es lo correcto: esa maquinaria es parte del apply, no de la
lectura.

**Podas concretas respecto del donante**, para que el diff de la Fase 2 sea legible:
- `win32_types.rs`: no viaja `AttachablePath` ni el campo `TopologySnapshot::attachable`.
- `enumerate.rs`: no viaja el segundo `for` de `seed_connected_inactive_displays` (el que cosecha
  los `attachable`) ni `query_active_only_topology` (base de los detach-only).

---

## ADR-003 — El motor migrado se **endurece** contra `panic = "abort"`

**Decisión**: `enumerate.rs` sale del donante con dos cambios de código, los únicos:
1. **Techo a los buffers** (`MAX_PATHS = 1024`, `MAX_MODES = 2048`). Windows dice cuánto reservar y
   el donante le creía: `vec![T::default(); n]`. Un `n` corrupto no devuelve `Err` — dispara
   `handle_alloc_error`, que **aborta el proceso**.
2. **Tope de reintentos** (`MAX_QUERY_ATTEMPTS = 8`). Ante `ERROR_INSUFFICIENT_BUFFER` el donante
   hacía `continue` sin contador: con la topología cambiando sin parar (una TV negociando HDMI, un
   dock enchufándose) el lazo no termina nunca. Acá corre dentro de `spawn_blocking`, así que
   colgaría un hilo del pool de Tokio para siempre.

**Por qué acá y no en Monarch**: Monarch compila con unwind; Millennium usa `panic = "abort"` en
release, así que **un panic en el módulo de monitores se lleva puesto el clipboard, el discovery y
las transferencias**. El resto del camino de lectura ya venía limpio: cero `unwrap`/`expect`/
indexing sin guarda. Estas dos eran las únicas bombas, y ninguna era un `unwrap`.

---

## ADR-004 — El comando existe en **toda** plataforma; lo que cambia es el cuerpo

**Decisión**: `displays_get_snapshot` se registra en el `generate_handler!` **sin `#[cfg]`**. Quien
decide por plataforma es `displays::snapshot()`: en no-Windows devuelve `Err`. Es el patrón que ya
usa `apply_update` en este codebase.

**Alternativa descartada**: gatear la entrada del `generate_handler!`. Funciona (verificado en la
fuente de `tauri-macros` 2.6.1: el parser lee atributos por entrada), pero en un build de Android el
`invoke` fallaría con `"Command displays_get_snapshot not found"` — un error de plomería en vez de
uno del dominio. Además volvería **load-bearing** el hecho de esconder el botón, que hoy es
cosmético.

**Corolario en el frontend**: el botón DISPLAYS se revela con `/android/i.test(navigator.userAgent)`,
la convención del codebase. **No** con `.desktop-only`: esa clase depende de `html.is-mobile`, que
`pre.js` activa también por ancho ≤ 900 px o pantalla táctil — una ventana angosta en Windows
perdería la feature.

---

## ADR-005 — Sin estado en `AppState`

**Decisión**: la Fase 1 no agrega ningún campo a `AppState` ni llama a `.manage()`. Consecuencia
directa del ADR-002: sin cache no hay estado que guardar.

**Por qué importa**: un campo gateado en `AppState` obliga a mantener el `#[cfg]` sincronizado en
**dos** lugares (la declaración del struct y el literal de `app.manage(...)`). Desincronizarlos da
`E0063` en Windows o un campo desconocido en Android — y el error solo aparece compilando para la
plataforma que no se probó. La Fase 2, cuando necesite estado, debe usar un `.manage()` propio
gateado (un solo punto de `cfg`) antes que un campo en `AppState`.

---

## ADR-006 — Los `u64` cruzan al frontend como **string**

**Decisión**: `adapterLuid` y `edidHash` se serializan como texto, no como número.

**Por qué**: superan `Number.MAX_SAFE_INTEGER` (2^53). JavaScript los redondearía **en silencio**,
y son justo los campos que definen la identidad de un monitor: dos pantallas distintas podrían
colapsar en el mismo id, que es la clave del render por diff.

---

## ADR-007 — El gate de Android es un `cargo check`, no un build de verdad

**Decisión**: `.github/workflows/android-cfg-gate.yml` corre `cargo check --target aarch64-linux-android`
en `ubuntu-latest`, **un solo ABI**, y nada más. Archivo aparte de `build.yml`.

**Qué protege**: que código windows-only se escape de su `#[cfg(target_os = "windows")]` y termine
compilándose en Android. Antes de esto, el único job era `windows-latest` — donde por definición TODO
el código de Windows compila y **una fuga no se nota**. El aislamiento estaba revisado por lectura y
nunca probado, justo cuando el árbol se llenaba de código Win32.

**Alternativa descartada — `tauri android build --debug`**: 20-40 min contra 2,1; arrastra JDK +
Gradle + SDK; y depende de `src-tauri/gen/android/`, que en este repo se edita a mano y es zona de la
regla dura *"NUNCA correr `tauri android init`"*. Para este propósito **no agrega señal**: una fuga de
`cfg` revienta en el type-check, mucho antes del linker o de Gradle.

**Un solo ABI alcanza**: el `cfg` que decide es `target_os`, que vale igual para los cuatro.
Chequear los otros tres es pagar 4× por la misma respuesta.

**El NDK no se descarga**: `ubuntu-latest` lo trae preinstalado en `ANDROID_NDK_ROOT`. Hace falta solo
porque `ring` (el proveedor cripto de rustls) compila C en su `build.rs` — o sea, `cargo check` **no**
es NDK-free, contra la intuición de que "check no linkea".

**Probado en falso, y esto es lo que lo separa de la decoración**: en una rama descartable se le sacó
el `#[cfg]` a `views_from_topology` y el gate se puso **rojo** (`E0433`). Con el código sano, verde.
**Un gate que nunca se vio fallar no es un gate.** Cualquiera que lo modifique debería repetir esa
prueba.

**Limitación declarada**: caza fugas de `cfg` en **Rust**. NO cubre regresiones de Gradle, del
manifest ni de Kotlin — eso sigue necesitando un build de Android de verdad, a mano.

---

## Doctrina CCD heredada — **PROHIBIDO simplificar** (para la Fase 2)

Esto todavía **no está implementado** acá: la Fase 1 no toca la topología. Queda escrito porque es
la razón de ser de Monarch, y quien escriba la Fase 2 tiene que traerlo entero. Origen:
`Monarch/docs/DECISIONS.md`, ADR-003/004/008/009.

- **Attach explícito, no "extender todo"** (Monarch ADR-003): conservar el `DISPLAYCONFIG_PATH_INFO`
  de los targets conectados-pero-inactivos que devuelve `QDC_ALL_PATHS` y **activar el target
  explícitamente** (`SDC_USE_SUPPLIED_DISPLAY_CONFIG`, source id libre per-adapter, todos los
  candidatos en un solo apply). *Cicatriz*: el path de la TV ya estaba enumerado y se tiraba a la
  basura "por prudencia", mientras el recovery le rogaba a Windows un extend que no podía funcionar.
- **Sondar Win32, no creerle a la documentación** (Monarch ADR-004): toda combinación de flags de
  `SetDisplayConfig` se valida con sondas `SDC_VALIDATE` **antes** de escribir código. *Cicatriz*:
  `SDC_APPLY | SDC_TOPOLOGY_EXTEND | SDC_ALLOW_CHANGES | SDC_SAVE_TO_DATABASE` es una combinación
  **ilegal** → error `87`. Costó meses.
- **Nunca juzgar por el código de retorno** (Monarch ADR-008): cada escalón se verifica
  **re-enumerando**. *Cicatriz*: un `SetDisplayConfig` sin cambios es un no-op documentado que
  devuelve `0`, y `SDC_VALIDATE` acepta hasta targets con `targetAvailable=FALSE` — un attach
  fantasma "exitoso" mataba el fallback.
- **El pre-estado es precondición dura, no `Option`** (Monarch ADR-009): sin captura del pre-estado
  (con retry), **no se toca la topología**. El tipo no es `Option`, así que el skip silencioso es
  imposible por construcción. *Cicatriz*: rollbacks `if let Some(pre) = ...` sin `else` → si la
  captura fallaba, el attach riesgoso corría **sin red y sin log**.
- **El watchdog de auto-rollback necesita las DOS piezas**: el manager del crate puro guarda el
  deadline (política) pero es **pasivo**; hace falta además la **glue** que dispare el rollback al
  vencer el timeout. Si la glue no existe, el layout malo queda pegado y nadie revierte — que es
  exactamente el bug de la TV irrecuperable que Monarch nació para matar.

---

## Nota de verificación — este proyecto **no tiene gate de compilación local**

Medido el 2026-07-20, no supuesto:

| Chequeo | Resultado |
|---|---|
| `cargo check` (toolchain default) | ❌ `error calling dlltool 'dlltool.exe': program not found` — el default es `stable-x86_64-pc-windows-gnu` y le falta dlltool; muere en `parking_lot_core`, antes de llegar al código del proyecto |
| `cargo check --target x86_64-pc-windows-msvc` | ❌ `linking with link.exe failed` — faltan las C++ Build Tools. Los build scripts se compilan igual para el host (gnu) |
| `cargo metadata` | ✅ resuelve — sirve para validar el manifest |
| `cargo test` en `vendor/monarch/` | ✅ **22 passed** — el crate puro es serde-only y sí linkea |
| `node --check src/*.js` | ✅ parsea |

O sea: para el **crate entero**, "compila" solo se puede afirmar desde el CI
(`.github/workflows/build.yml`). El `CLAUDE.md` que dice *"`cargo check` es el primer paso ante
cualquier problema"* está desactualizado.

### Pero sí hay gate local para el módulo de displays: el harness aparte

Dato que importa y que no era obvio: **el que necesita `dlltool` NO es el crate `windows`** — es
`parking_lot_core`, una dependencia transitiva de Tauri. `windows 0.60` usa `windows-link`/raw-dylib
y **el toolchain gnu lo chequea sin problema**.

Así que el módulo de displays se puede verificar localmente en un crate scratch con las **mismas
dependencias reales** (`windows 0.60` con las mismas features + el path-dep real a
`src-tauri/vendor/monarch` + serde) y un `mod runtime_log` de mentira con `info/warn/err`. Copiando
ahí `displays/{mod,enumerate,win32_types}.rs` verbatim:

- `cargo check` ✅ → **type-checkea el módulo entero, las dos ramas de `cfg`**.
- `cargo test` ❌ → **linkear** el binario de test sí pide `dlltool`. Para correr los tests de lógica
  pura hay que armar una segunda variante sin la dependencia `windows` (cambiándole el `target_os`
  del `cfg` para apagar los bloques Win32).

Con eso, la Fase 1 se verificó ANTES del CI: check verde en ambas ramas, 4 tests de lógica en verde,
22 tests del vendor en verde. **Usar esta técnica en las Fases 2 y 3** — es mucho más barato que
gastar una corrida de CI de 11-30 minutos para descubrir un error de tipos.

Arreglo de fondo si alguna vez se quiere el gate local completo: instalar los binutils de mingw-w64
(para tener `dlltool.exe`) **o** las C++ Build Tools de Visual Studio.
