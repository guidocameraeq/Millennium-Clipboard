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

## Doctrina CCD heredada — **PROHIBIDO simplificar**

**Estado: IMPLEMENTADA en la Fase 2** (2026-07-20). Los cinco puntos de abajo viven hoy en
`src/displays/{apply,topology,watchdog}.rs` + la glue de `mod.rs`. Sigue acá, en presente, porque es
la razón de ser de Monarch y porque cualquiera que toque este módulo tiene que poder leerla sin ir
al repo donante. Origen: `Monarch/docs/DECISIONS.md`, ADR-003/004/008/009.

Dónde vive cada punto hoy:

| Punto | Implementación |
|---|---|
| Attach explícito | `topology::try_batch_explicit_attach` + `apply::build_attach_paths`, alimentados por `TopologySnapshot::attachable` (cosechado de `QDC_ALL_PATHS` en `enumerate.rs`) |
| Sondar Win32 | `apply::validate_attach_paths` (`SDC_VALIDATE`) antes de cada `apply_attach_paths`, batch creciendo de a uno |
| Verificar re-enumerando | `topology::settle_poll` + `apply_layout_against_snapshot` (re-enumera post-SDC) + la verificación propia de Millennium en `mod.rs::estado::toggle` |
| Pre-estado como precondición dura | `topology::capture_pre_recovery_state` (devuelve `Result`, no `Option`) |
| Watchdog con sus DOS piezas | manager del vendor (pasivo) + `watchdog::correr` (el gatillo) — ver **ADR-009** |

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

## ADR-008 — La Fase 2 trae el motor entero **menos la persistencia binaria del snapshot**

**Decisión**: `topology.rs` se portó completo —cache en `Mutex`, todos los merges, el remapeo de
`DisplayId` y la escalera de rescate de 4 escalones— **salvo** `PersistedRawSnapshot`,
`persist_raw_snapshot`, `load_persisted_raw_snapshot`, `persisted_raw_snapshot_path`,
`struct_to_bytes` y `struct_from_bytes`. O sea: no existe `topology_snapshot.json`.

**Dos motivos, los dos graves**:
1. `struct_from_bytes` hacía `MaybeUninit::assume_init()` sobre bytes leídos **de un archivo de
   disco**, validando solo el largo. Los structs que salen de ahí van derecho a `SetDisplayConfig`.
   Con `panic = "abort"` y una API que reconfigura pantallas, es la bomba más grande del donante —
   y es exactamente la clase de código que el ADR-002 ya había celebrado no traer.
2. `persisted_raw_snapshot_path()` resolvía vía `monarch::FileConfigStore::default_config_path()`, o
   sea escribía en **`%APPDATA%\Monarch\` — la configuración real de Monarch del usuario**. Era el
   *segundo* camino hacia esa carpeta, además del store del manager (ver ADR-010); el que estaba
   señalado era el otro.

**Qué se pierde, dicho sin maquillaje**: el recuerdo de un monitor detachado **no sobrevive a
reiniciar Millennium**. Dentro de la sesión el cache en memoria hace el mismo trabajo, y los
candidatos de attach salen de `snapshot.attachable`, o sea de la enumeración **viva**
(`QDC_ALL_PATHS`) y nunca del archivo — el propio donante lo dice en un comentario. Si algún día
aparece el caso "apagué la TV, cerré la app, la abrí y no la puedo prender", **esto es lo primero a
mirar**, y la cura correcta sería re-agregar la persistencia con un parseo que valide campo por
campo, no un `assume_init`.

**Consecuencia manejada**: se eliminó el campo `reseed_persisted` del cache en vez de neutralizarlo
(sin archivo no hay de dónde re-sembrar; un campo que nadie lee es código muerto disfrazado).
`invalidate_cache` sigue haciendo lo que importa: vacía el cache en memoria, que es lo que llama el
resume-listener.

---

## ADR-009 — El watchdog de auto-rollback: por qué **no** es una copia literal del donante

**Decisión**: la lógica del watchdog se reescribió en `displays/watchdog.rs` en vez de copiar
`Monarch/src-tauri/src/app/events.rs::spawn_confirmation_watchdog`. Tres diferencias, todas a favor
de que efectivamente dispare:

1. **Margen sobre el plazo.** El donante duerme *exactamente* el timeout y recién ahí pregunta
   "¿venció?". `thread::sleep` e `Instant::elapsed` usan el mismo reloj monotónico, así que en la
   práctica da verdadero — pero es una carrera sin red: si alguna vez despierta un microsegundo
   antes, la respuesta es "todavía no", **el hilo se muere ahí y nadie vuelve a preguntar nunca**.
   El costo de esa carrera es la máquina inusable; el costo de cubrirla son diez líneas.
2. **Lazo de reintento acotado** (5 vueltas). Cubre la carrera de arriba *y* el caso feo de verdad:
   que el rollback **falle**. El manager, cuando eso pasa, **conserva** la confirmación pendiente en
   vez de tirarla (`manager.rs:185-190`), así que reintentar tiene sentido. El donante se iba en el
   primer error.
3. **Decisión separada del efecto.** `decidir()` y `correr()` no mencionan a Tauri ni al crate
   `windows`; el hilo y los eventos se inyectan. No es purismo: es lo que permite **testear la red
   de seguridad de verdad** (ver ADR-011). El donante tenía el `AppHandle` adentro, y por eso su
   watchdog no tenía un solo test.

**Invariante que sostiene todo el diseño, y que hay que respetar al tocar `estado::toggle`**: una vez
aplicado el cambio, **ninguna salida puede irse sin dejar armado el watchdog** (salvo que ya no
quede confirmación pendiente). Por eso en ese tramo no hay un solo `?`: un `?` es un `return`
silencioso que deja el cambio puesto y a nadie persiguiéndolo. Los tres caminos que lo respetan:
la verificación falla → rollback inmediato; el rollback inmediato falla → watchdog igual;
**no se pudo ni re-enumerar** → watchdog igual (es lo conservador: si el usuario ve bien su
pantalla, confirma; si no, vuelve sola).

**Un watchdog viejo no puede pisar un apply nuevo**, y no hace falta un contador de generación para
eso: el manager solo revierte lo que está **vencido**, y un pendiente recién creado no lo está. Está
testeado (`un_watchdog_viejo_no_revierte_un_cambio_nuevo`).

---

## ADR-010 — El store apunta al APPDATA de Millennium, y hay un test que lo vigila

**Decisión**: `MillenniumConfigStore` (nuevo, `displays/store.rs`) implementa `monarch::ConfigStore`
sobre el `JsonStore` atómico de Millennium (tmp + rename, backup-on-corrupt) y escribe en
`%APPDATA%\com.guidocameraeq.millennium\displays.json`. **`FileConfigStore` no se instancia nunca.**

**Qué evita**: su `default_config_path()` apunta a `%APPDATA%\Monarch\config.json` — los perfiles
reales que el usuario tiene en la otra app. Y escribe con `fs::write` directo, no atómico: un corte
a mitad de escritura deja un JSON truncado, la clase de bug que Millennium ya había arreglado.

**El `data_dir` entra desde afuera** (`setup()` lo saca de Tauri) y **no hay** un constructor que lo
resuelva por su cuenta: cuantas menos formas existan de construir este store, menos formas hay de
que una termine apuntando a otra carpeta.

**La red**: el test `la_ruta_cae_en_millennium_y_nunca_en_monarch` falla si la ruta llega a contener
"monarch". No es decorativo — corre en CI (ADR-011).

---

## ADR-011 — Los tests del proyecto no corrían en **ningún** lado; ahora sí

**Hallazgo, medido el 2026-07-20**: los `#[cfg(all(test, not(windows)))]` que el repo venía
acumulando (los de `json_store.rs`, los 4 de `displays/mod.rs`) **no se ejecutaban ni local ni en
CI**. Local no, porque para un target no-Windows falta el linker; y en CI tampoco, porque
`build.yml` **nunca invocó `cargo test`**. Eran decoración: verdes por no existir.

**Decisión**: `src-tauri/displays-tests/` — un crate chico, **fuera del workspace** (`[workspace]`
vacío, mismo patrón que `vendor/monarch`; verificado con `cargo metadata` que
`workspace_members = [millennium-clipboard]` y que el `panic = "abort"` no se movió). Incluye por
`#[path]` los archivos **reales** que son windows-free por diseño (`watchdog.rs`, `store.rs`,
`ids.rs`) más `json_store.rs`, con dobles de `runtime_log` y `diagnostics`. Sin `windows` ni `tauri`,
el binario de tests **linkea**. Corre en `.github/workflows/displays-tests.yml`, en `windows-latest`.

**Por qué windows-latest y no ubuntu**: los archivos incluidos llevan `#![cfg(target_os = "windows")]`
adentro. En Linux se compilarían **vacíos** y el job daría verde sin haber corrido un solo test —
exactamente el gate decorativo que esto viene a eliminar. Por la misma razón el workflow tiene un
paso que **exige un mínimo de tests ejecutados** y falla si corrieron menos.

**Consecuencia de diseño, no accidental**: `watchdog.rs`, `store.rs` e `ids.rs` **no pueden** empezar
a usar el crate `windows` ni `tauri`. Si alguna vez hace falta, la lógica testeable se extrae antes.
Ese es el precio de tener probada la red de seguridad, y vale la pena: sin esto, la única evidencia
de que el auto-rollback funciona sería que alguien lo corrió a mano una vez.

---

## ADR-012 — El apply de **layout completo** (perfil y lienzo) NO compara ids a nivel glue

**Estado: Fase 3.** `aplicar_con_red` (la red compartida por `cargar_perfil` y el lienzo) **no hace
una comparación de ids** para decidir si el cambio "tomó": arma el watchdog, emite el evento y
devuelve la foto fresca, y listo. La verificación por re-enumeración queda en manos del backend
(`settle_poll`, que corre dentro del apply y conoce el remapeo por EDID) y el auto-revert real es el
watchdog (si el usuario no confirma, vuelve solo).

**La alternativa que se probó y se descartó** (la cazó la revisión adversarial antes de commitear): que
`cargar_perfil` leyera los ids que "tienen que quedar activos" y `aplicar_con_red` los verificara
re-enumerando, en paralelo a lo que hace `toggle`. Dos bugs, opuestos, los dos reales:

1. **Falso negativo.** Los ids "esperados" se leían de `guard.get_layout()` *después* del apply, que
   re-enumera el estado REAL, no el target. Un monitor que el perfil pidió prender y que NO prendió
   simplemente no aparecía en "esperados", así que el chequeo "¿están todos los esperados activos?"
   pasaba siempre: incapaz de cazar el no-op-que-devuelve-éxito que dice cazar.
2. **Falso positivo.** "Esperados" salía de `get_layout()` (que pasa por `merge_layout_with_fresh`,
   que **rellena el EDID desde el cache** → id `luid:target:HASH`) y se comparaba contra `foto_cruda`
   (que usa el EDID **crudo** de la enumeración → si leyó `None`, id `luid:target:none`). Los ids de la
   misma pantalla no matcheaban → se revertía un apply que SÍ había andado. Sumado a la carrera de
   asentado en un detach (un monitor apagándose enumera "prendido" un instante y "ausente" el
   siguiente), el rollback espurio era probable justo en la ventana post-cambio.

**Por qué `toggle` sí puede comparar ids y esto no**: el target de `toggle` es UN monitor conocido,
leído del estado **pre-apply** por la MISMA ruta de enumeración en ambos lados (`foto_cruda` antes y
después). Un layout completo con remapeo por EDID no tiene ese lujo: cualquier comparación de ids a
nivel glue cruza rutas distintas. La lección: **no re-verificar arriba lo que el backend ya verifica
bien abajo.** `aplicar_con_red` es un paralelo del bloque post-apply de `toggle` en lo único que
importa (el invariante "ninguna salida sin dejar el watchdog armado"), no en la parte de la
comparación.

---

## ADR-013 — El watcher en vivo usa **dos canales**: `WM_DISPLAYCHANGE` refresca, solo el resume invalida

**Estado: Fase 3.** La ventana oculta de `system_events.rs` (que la Fase 2 usaba solo para el resume)
ahora atiende también `WM_DISPLAYCHANGE`, pero por un **canal propio** (`canal_cambio`), con su propio
hilo consumidor y su propio callback. El callback del cambio solo emite `displays-changed` (el frontend
re-consulta y re-enumera fresco); el callback del resume, aparte, es el único que llama
`invalidate_backend_cache`.

**Por qué separados y no un solo camino**: invalidar el cache ante `WM_DISPLAYCHANGE` es la cicatriz
que Monarch dejó anotada. El cache es lo que mantiene vivo el recuerdo de un monitor **detachado**, y
un apply propio dispara `WM_DISPLAYCHANGE` — invalidar ahí borraría el monitor que se acaba de apagar y
con él la posibilidad de volver a prenderlo. Un solo camino compartido tarde o temprano invalidaría de
más. Con dos canales, "refrescar la vista" y "tirar el cache" no se pueden cruzar por accidente.

**Alternativa descartada** — un canal con un enum de razón (`Resume` | `Cambio`) y un solo consumidor:
funciona, pero el resume y el cambio quieren *debounces distintos* (2000 ms el resume, para absorber la
ráfaga de despertar; ~400 ms el cambio, para reaccionar rápido al enchufe), y mezclarlos en un
consumidor es más frágil que dos hilos cortos que bloquean en su `recv`. **CPU en reposo intacto**: los
dos consumidores bloquean en `recv_timeout` y la ventana en `GetMessageW`; cero poll.

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
`src-tauri/vendor/monarch` + serde) y un `mod runtime_log` de mentira con `info/warn/err`.

**Mejora de la Fase 2 — nada de copiar: `#[path]`.** La Fase 1 copiaba los archivos al scratch, que
es una copia que se desincroniza en cuanto alguien edita el original. Ahora el scratch los **incluye
en su lugar**:

```rust
// scratch/src/lib.rs
pub mod runtime_log { /* doble con la MISMA firma: info/warn/err, impl Into<String> */ }
#[path = "<abs>/src-tauri/src/json_store.rs"] pub mod json_store;
#[path = "<abs>/src-tauri/src/displays/mod.rs"] pub mod displays;
```

y el `Cargo.toml` del scratch espeja el bloque `[target.'cfg(target_os = "windows")'.dependencies]`
del proyecto. Con eso, lo que se chequea **es** el código que va al binario.

**Dos ajustes al espejar, verificados en la Fase 3 (2026-07-21):** (1) **sacar `winreg`** del bloque
windows — displays no lo usa, y `winreg` arrastra `windows-sys → windows_x86_64_gnu`, cuyo
build-script **pide `dlltool`** (ausente acá); con winreg adentro la rama Windows del scratch muere en
ese build-script antes de tocar el código de displays. (`windows 0.60` es raw-dylib/`windows-link` y
`cargo check` no linkea, así que ese sí pasa sin `dlltool`.) (2) **agregar `anyhow`** a
`[dependencies]`: lo usa `json_store.rs`, que entra por `#[path]`.

Las dos ramas de `cfg`, las dos verificables local:

| Comando | Qué prueba |
|---|---|
| `cargo check` | rama Windows: el motor CCD entero |
| `cargo check --target x86_64-unknown-linux-gnu` | **rama no-Windows: caza fugas de `cfg` sin gastar CI** (requiere `rustup target add x86_64-unknown-linux-gnu`; `check` no linkea, así que no hace falta un compilador de C) |

Ese segundo comando es el gate de Android **local**: da la misma respuesta que el workflow de
`aarch64-linux-android` en 2 segundos en vez de 2 minutos, porque el `cfg` que decide es `target_os`.

`cargo test` en ese scratch **sigue fallando** (linkear con `windows` pide `dlltool`). Para eso está
`src-tauri/displays-tests/`, que ya vive en el repo y corre en CI — ver **ADR-011**.

Con esta técnica, la Fase 2 se verificó ANTES del CI: check verde en ambas ramas y **sin una sola
advertencia**, 13 tests de lógica en verde, 22 del vendor en verde, `node --check` OK. **Usar esto en
la Fase 3** — es mucho más barato que gastar una corrida de CI de 11-30 minutos para descubrir un
error de tipos.

### Addendum al ADR-003 — endurecimientos que se sumaron en la Fase 2

El ADR-003 decía "dos cambios de código, los únicos". Ya no son dos; quedan anotados acá para que el
diff contra el donante siga siendo auditable. Ninguno cambia comportamiento observable:

- `apply.rs::gamma_ramp_looks_identity` — `ramp[base + i]` → `.get()`. El índice máximo es 767 sobre
  un array de 768, o sea era seguro; se cambió por consistencia con la regla de "cero indexing crudo".
- `enumerate.rs::query_active_topology` — el error de `query_raw_database_current` se registra en
  `stats.discarded` en vez de descartarse con `.ok()`. Mismo control de flujo, más rastro.
- `topology.rs::settle_poll` — `missing[0]` → `.first()`, y un `MAX_SETTLE_ATTEMPTS = 64` **además**
  del plazo por reloj (el peor caso legítimo son 14 vueltas). Regla del proyecto: ningún bucle de
  reintento sin contador.
- `topology.rs::choose_remap_candidate` — `candidates[0]`/`enumerated[0]`/`enabled[0]` → `.first()`.
- `apply.rs` — el mensaje de error de `SetDisplayConfig` dejó de estar escrito a mano en dos archivos.
  Ahora la constante y el reconocedor del **error 87** viven juntos en `apply.rs` y `topology.rs` los
  usa. Antes, cambiar ese `format!` apagaba la escalera de rescate **en silencio**.

Arreglo de fondo si alguna vez se quiere el gate local completo: instalar los binutils de mingw-w64
(para tener `dlltool.exe`) **o** las C++ Build Tools de Visual Studio.
