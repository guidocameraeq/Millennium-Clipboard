# CHANGELOG — Millennium Clipboard

> Historia permanente. `/cierre` agrega una entrada AL TOPE en cada sesión. Orden descendente estricto, sin excepciones. Nada de versiones duplicadas en otros docs.

## 2026-07-20 (b) — Gate de Android en el CI: cierra el hueco que dejó la Fase 1

El aislamiento de plataforma pasa de **revisado por lectura** a **probado**. Hasta acá el único job era `windows-latest`, donde por definición TODO el código windows-only compila y una fuga de `cfg` no se nota; recién se hubiera visto intentando un build de Android a mano, meses después. Verificado **contra la API de GitHub**, no contra el reporte de la sesión que lo construyó.

### Added
- **`.github/workflows/android-cfg-gate.yml`** (nuevo, archivo aparte — **no se tocó `build.yml`**): corre en `ubuntu-latest` y hace una sola cosa, `cargo check --target aarch64-linux-android`. No construye la app; type-checkea, que es todo lo que hace falta para cazar código windows-only escapado de su `#[cfg(target_os = "windows")]`.
  - **Probado en falso** (lo que lo separa de la decoración): en una rama descartable se le sacó el `#[cfg]` a `views_from_topology` y el gate se puso **rojo** (`E0433: cannot find module win32_types`); con el código sano, **verde**. Runs: `2955104` ✅ (2,1 min) · `488b4c4` ❌. `Build Windows` @ `2955104` ✅ — no se rompió nada.
  - **Costo**: 2,1 min, y el **NDK no se descarga** (`ubuntu-latest` lo trae en `ANDROID_NDK_ROOT`; hace falta solo porque `ring` compila C en su `build.rs`). Corre **en paralelo** al job de Windows ⇒ el CI no tarda más en pared.
  - **Descartado `tauri android build --debug`**: 20-40 min, arrastra JDK + Gradle + SDK, y depende de `src-tauri/gen/android/` — zona de la regla dura "NUNCA correr `tauri android init`". No agrega señal acá: una fuga de `cfg` revienta en el type-check, mucho antes del linker o de Gradle.
  - **Un solo ABI alcanza**: el `cfg` que decide es `target_os`, idéntico para los cuatro.
  - **Limitación declarada**: caza fugas de `cfg` en **Rust**. NO cubre regresiones de Gradle/manifest/Kotlin.

### Changed
- **`.github/workflows/build.yml`** — el comentario decía que era "el ÚNICO gate de compilación del proyecto"; ya no lo es. Ahora aclara que es el único que **compila y linkea de verdad**, y que el de plataforma vive aparte.

## 2026-07-20 (a) — SPEC-displays Fase 1 (ver los monitores, read-only) IMPLEMENTADA + VERIFICADA EN HARDWARE REAL

Se migró de **`guidocameraeq/Monarch` @ `7f9f63b`** (el fork de Guido, NO el upstream) el camino de **lectura** del motor CCD, y se expuso en un modal propio. **Cero `SetDisplayConfig`: el motor de apply no se copió, no existe en el repo** (verificable con grep — los únicos hits del nombre son comentarios). Diff aditivo; núcleo de Millennium (clipboard/discovery/HTTPS/transferencias/pinning) **intacto**. **Cerró también el pendiente físico de la Fase 0.** Verificado con **CI verde** (run [29754851028](https://github.com/guidocameraeq/Millennium-Clipboard/actions/runs/29754851028), 6,5 min, 12/12 pasos `success`, `.exe` 4,19 MB) **y con la prueba física del usuario en el desktop de 3 displays**: aparecen los 3 monitores reales, **incluida la desconectada**, y el uso diario siguió igual. Review adversarial previo al push (5 lentes, 23 agentes) → **7 hallazgos reales, todos corregidos**.

### Added
- **Vendor del crate puro `monarch`** (`src-tauri/vendor/monarch/`, 10 archivos + `PROVENANCE.md`) — copia byte a byte de `Cargo.toml` + `src/` + `LICENSE` desde el fork de Guido, commit `7f9f63b`. MIT íntegro con **sus dos** copyrights (Nuzair46 upstream + Guido fork). Path-dep bajo `[target.'cfg(target_os = "windows")'.dependencies]` ⇒ Android no lo ve. Verificado que un crate anidado **NO** se vuelve miembro implícito de workspace (`cargo metadata` ⇒ `workspace_members = [millennium-clipboard]`), así que el `[profile.release] panic="abort"` no se mueve.
- **Motor de enumeración** (`src-tauri/src/displays/{enumerate,win32_types}.rs`) — windows-only con doble gate (`#[cfg]` en el `mod` + `#![cfg]` interno). Solo ejecuta `GetDisplayConfigBufferSizes`, `QueryDisplayConfig` y `DisplayConfigGetDeviceInfo`. Hace visibles los monitores **conectados-pero-apagados** vía `QDC_ALL_PATHS` — la razón de ser de la fase.
- **`displays_get_snapshot`** (`lib.rs`) — async + `spawn_blocking`, **sin `cfg` en el `generate_handler!`**: el comando existe en toda plataforma y el que decide es el cuerpo (fuera de Windows, `Err`). Patrón de `apply_update`; gatear la entrada del handler haría que en Android el `invoke` falle con "Command not found" y volvería *load-bearing* esconder el botón.
- **Frontend de displays** — botón HUD `DISP` (revelado por `/android/i.test(userAgent)`, **no** por `.desktop-only`) + `#displays-modal` espejando el molde del modal LOG + lista con render por diff (molde `buildPeerItem`/`updatePeerItem`) y badges PRIMARY/ACTIVE/DETACHED. Los `u64` (`adapterLuid`, `edidHash`) viajan como **string**: superan 2^53 y JS los redondearía justo en los campos que definen la identidad del monitor.
- **`docs/DECISIONS.md`** (nuevo) — 6 ADRs, la **doctrina CCD heredada** de Monarch (ADR-003/004/008/009: attach explícito · sondar Win32 en vez de creerle a la doc · verificar re-enumerando · pre-estado como precondición dura · el watchdog necesita sus dos piezas) marcada **PROHIBIDO simplificar** para la Fase 2, y la nota de verificación del proyecto.
- **Modo de monitores falsos** — `MONARCH_FORCE_MOCK_BACKEND` (heredado de Monarch) devuelve 3 monitores de mentira con cartel amarillo en la UI; sirve para ver la pantalla en cualquier plataforma.

### Changed
- **`src-tauri/Cargo.lock`** — commiteado con `windows 0.60` + `monarch`. **Era un agujero de la Fase 0**: el lock no reflejaba el `Cargo.toml`.
- **Grilla del HUD** (`styles.css`) — de `repeat(4, 1fr)` a `repeat(auto-fit, minmax(64px, 1fr))` en las dos reglas de mobile, por el 5º botón. `auto-fit` cubre los dos casos (4 botones en Android, 5 en Windows).
- **`.github/workflows/build.yml`** — el comentario ahora explica que es el único gate de compilación del proyecto y qué prueba cada fase.

### Fixed
- **El botón `DISP` se veía en Android pese al atributo `hidden`** (hallazgo del review). El `[hidden]{display:none}` es regla de **user-agent** y pierde contra **cualquier** declaración de autor: `.hud-btn` declara `display:inline-flex`, y `html.is-mobile .hud-btn` encima usa `!important`. Sin la regla nueva `.hud-btn[hidden] { display: none !important; }`, el gate por userAgent no ocultaba nada y el `invoke` se disparaba igual. El codebase ya había tropezado con lo mismo (`.backend-banner[hidden]`, `.qr-pane[hidden]`, `.dropzone-count[hidden]`).
- **Techo de buffers demasiado bajo** (1024/2048 → **65 536/131 072**). `QDC_ALL_PATHS` es **combinatorio** (una entrada por cada source×target de cada adaptador): con iGPU + dGPU + adaptadores virtuales se pasa. Y el `Err` autoinfligido se lo tragaban el `let ... else` del seeder y el `.ok()` del enriquecimiento ⇒ el modo de falla era **"la TV desconectada no aparece", sin un solo log**.
- **Descartes silenciosos de errores de enumeración** — `QDC_ALL_PATHS` y `QDC_DATABASE_CURRENT` ahora dejan rastro (`all-paths-fail:` / `db-current-fail:`) en la línea `enum:` del runtime log, en vez de perderse.
- **Dos badges PRIMARY en modo espejo** — el motor deduce `is_primary` de la posición (0,0) y dos monitores clonados la comparten. Se desempata en la vista (`keep_single_primary`), sin tocarle la semántica al motor migrado.
- **Endurecimiento contra `panic = "abort"`** — techo a los buffers que dimensiona Windows (un `vec!` con `n` corrupto dispara `handle_alloc_error` y **aborta el proceso**, no devuelve `Err`) y tope de reintentos en el lazo de `QueryDisplayConfig` (el del donante podía colgar un hilo del pool de Tokio para siempre si la topología cambiaba sin parar).

### Removed
- **`mod ccd_link_smoke`** (`lib.rs`, Fase 0) — era un canario de linkeo; el motor real llama la misma familia de funciones, así que ya no aporta nada.
- **Del motor migrado, respecto del donante**: `apply.rs` (845 líneas) y `topology.rs` (1331) **no viajaron**. `topology.rs` importa 12 símbolos de `apply.rs` y su `new()` llama `capture_sdr_gamma_ramps` ⇒ copiarlo arrastraba el apply entero. Y no hacía falta: su andamiaje (cache, merges, persistencia) existe para **re-adjuntar**, no para leer. Con él se fueron el `assume_init` sobre bytes de un archivo de disco, el acople a `%APPDATA%\Monarch\config.json` (el config real de Monarch del usuario) y el único `eprintln!`. También se podaron `AttachablePath`, el campo `TopologySnapshot::attachable` y `query_active_only_topology`, todos material de apply.

## 2026-07-18 — SPEC-displays Fase 0 (CI + link smoke de `windows 0.60`) IMPLEMENTADA + VERIFICADA (CI verde)

Arrancó `docs/SPEC-displays.md` (migrar el motor de monitores de Monarch a Millennium, 4 fases). Se ejecutó **solo la Fase 0** — el prerequisito bloqueante: montar CI y probar que `windows 0.60` **linkea** en el runner MSVC. Diff **aditivo + gateado a `cfg(windows)`**; núcleo de Millennium (clipboard/discovery/HTTPS/transferencias/pinning) **intacto**. **Verificado con el CI VERDE** (run [29650684956](https://github.com/guidocameraeq/Millennium-Clipboard/actions/runs/29650684956), 11,4 min, todos los pasos `success`, artefacto `.exe` 4,2 MB) → **el riesgo [ALTO] del SPEC (que `windows 0.60` no linkee sin MSVC local) queda retirado**. Review adversarial previo al push (4 lentes: ci-yaml / rust-compile / android-cfg-leak / link-proof) → 4× would-pass, 0 blockers; único nit aplicado: `timeout 60→90`.

### Added
- **CI de Windows** (`.github/workflows/build.yml`) — portado del `build-personal.yml` de Monarch, adaptado a Millennium: **npm** (no yarn/vite), **sin instalador** (`bundle.active=false`), `frontendDist ../src`, target `x86_64-pc-windows-msvc` (crt-static ya en `.cargo/config.toml`), artefacto `millennium-clipboard.exe`. Trigger: push a `feat/displays` + `workflow_dispatch`; cache de Rust (Swatinem); `timeout: 90` (la 1ra corrida en frío compila las dos versiones del crate `windows`).
- **`windows = 0.60`** (`src-tauri/Cargo.toml`) con 10 features CCD (enumerate/apply/topology) **SOLO bajo `[target.'cfg(target_os = "windows")'.dependencies]`** → Android no lo ve. Pin 0.60 (convive con el 0.61.x transitivo de tauri/wry, dos árboles paralelos).
- **Smoke de linkeo** (`src-tauri/src/lib.rs`, `mod ccd_link_smoke`) — llama `GetDisplayConfigBufferSizes` (función CCD raw-dylib REAL, mismo patrón que Monarch) desde `run()` y loguea el status por `runtime_log::info`. **Sin `unwrap`/`expect`** (respeta `panic=abort`), aditivo y no-fatal. Un dep sin usar no emite el import raw-dylib → se referencia de verdad para que el CI pruebe el link. **La Fase 1 lo reemplaza** por el backend migrado.
- **`docs/SPEC-displays.md`** (commit `a7224b3`) — el plan de la migración (SPEC delta): AGREGA/MODIFICA/NO SE TOCA, 4 fases (0 CI · 1 ver monitores · 2 apply con red de seguridad · 3 perfiles/watcher/lienzo), criterios de aceptación por fase, riesgos (watchdog de auto-rollback, fuga de `cfg`, `panic=abort`, licencia MIT).

## 2026-07-15 — Pulido de UI (SPEC-ui-polish) IMPLEMENTADO (T1–T6)

Se ejecutó entero `docs/SPEC-ui-polish.md` (ahora en `docs/archive/`). **Solo frontend** (`src/index.html`, `src/main.js`, `src/styles.css`); backend Rust y motor de transferencia **intactos**. Verificado E2E manejando el webview real por **CDP** (WebView2 remote-debugging): capturas + `Runtime.evaluate` + lectura de consola → **0 errores, 0 violaciones de CSP** en todos los estados. **8/9 criterios verificados**; el #1 solo en su parte de consola/CSP (el round-trip físico de transferencia necesita 2 PCs → pendiente, riesgo casi nulo). Review adversarial (5 lentes + verificación) → 0 bugs de correctitud / NO SE TOCA / escaping; 4 hallazgos cosméticos bajos, limpiados.

### Added
- **Estado de la cola dentro del cuadro (T4)** — `#dropzone-count` "N archivo(s) listo(s)"; `.file-queue` con `max-height:168px` + scroll interno (`flex:none`) → varios archivos ya no empujan TRANSMIT (mobile: `40vh`).
- **Foco accesible (T3)** — `:focus-visible` (anillo cian) en botones, textarea y switches (con override id para `#text-composer` que traía `outline:none`); los switches vuelven al tab-order (visually-hidden en vez de `display:none`, manteniendo el `input:checked + .sound-track`); `focusFirstControl()` mete el foco al 1er control al abrir los 5 modales (settings, log, qr, peer-details, add-peer).
- **`activateMode()` (T2)** — extraída del click de los `.mode-btn`, reutilizable programáticamente.

### Changed
- **Config colapsable (T5)** — las 7 secciones planas (`h3.settings-section`) → 4 grupos `<details class="settings-group">/<summary>` (GENERAL abierto; TRANSFERS & NOTIFICATIONS y SYSTEM `.desktop-only`; UPDATES). Nativo, **sin JS** (CSP-safe, confirma el supuesto MEDIO del spec). Los 17 ids que lee el JS, preservados; frontera Android intacta.
- **`renderQueue` reescrito (T4)** — `createElement` + `textContent` (sin `innerHTML`); estado en el cuadro; el tamaño solo si `>0` (mata el "0 B"); botón quitar `<a>[X]` → `<button class="queue-remove" aria-label>` (el handler delegado `[data-remove]` sigue matcheando).
- **Auto-selección al abrir (T3, D1)** — primer peer **VISIBLE según el filtro** (espeja el predicado de `renderPeers`: favoritos→`p.favorite`, si no→`p.status!=='offline'`), no `state.peers[0]` a secas → ya no "traba" a un peer que no está en la lista visible. Toca SOLO la línea de auto-selección, no `renderPeers`.
- **Contraste de etiquetas (T6, D2, preview aprobado)** — `--text-dim` `#455d70 → #607c8f` (3.0:1 → 4.7:1, cumple WCAG AA). El dueño eligió la opción B tras ver el antes/después (artifact).
- **Drag&drop activa modo FILE (T2)** — `tauri://drag-drop` llama `activateMode('file')` si cayó ≥1 archivo. Antes, soltar en modo TEXT dejaba `#mode-file` oculto y `transmit()` mandaba texto vacío ("empty payload").
- **Textos de modales (T1)** — 5 párrafos con jerga (mDNS, fingerprint/port, "MANUAL + favourite") reescritos a criollo corto (add-peer por IP, forget peer, QR mostrar/escanear/pegar).
- **Cartel CTRL+ENTER (limpieza review)** — `.composer-meta` a `justify-content:flex-end` para que el hint quede a la derecha tras sacar el contador.

### Removed
- **Ruido visual (T1)** — UPTIME (nodo HUD + ticker + const), 2 slogans del placeholder, `PROTO mDNS+HTTPS`, el contador `0000 CHARS` (nodo + `updateCharCount` + const + listener + 2 call-sites), la fila falsa **DATA DIR** (`settings-data-dir`, vía T5). El hex bajo "TRANSMIT TO" se oculta por CSS (sigue en cada peer → no se pierde desambiguación). La regla CSS mobile del PROTO se reapuntó a `nth-child(4)` para no destapar el toggle CLACK.
- **Código muerto** — `escapeHtml()` (sin uso tras reescribir `renderQueue`) y las reglas CSS `.settings-section` huérfanas tras T5.

## 2026-07-15 — Fase 2: verificación física Bloque A (datos) OK + auditoría de UI + SPEC de pulido READY

Sesión de **verificación + auditoría + spec**; **sin cambios de código** (`src/` y `src-tauri/` intactos). Todo lo de datos se probó con una **instancia aislada** (`MILLENNIUM_INSTANCE=verif`, `MILLENNIUM_PORT=53400`), **sin tocar los datos reales**.

### Verificado (físico, por el usuario)
- **Fase 2 · Bloque A (Datos) — OK**: (1) un favorito sobrevive a un kill **forzado** — escritura atómica (sin `.tmp` residual), archivo íntegro tras el crash, `[prefs] loaded 1 favorite(s)` al reabrir; (2) un `prefs-verif.json` corrompido a mano → `ERR [jsonstore] parse failed` + `prefs-verif.json.corrupt` con el dato recuperable + reset a default. Dos matices honestos anotados (el archivo original queda corrupto hasta el próximo write; `prefs` no muestra aviso visual de corrupción, solo el log → TODO 🟢).
- **Fase 2 · Bloque B (UI) — 1/5 OK**: "texto entrante sobrevive un ACK" (`#incoming-toast` y `#toast` conviven). Faltan 4 (necesitan 2 PCs): TARGET LOST, error que no se pisa a los 5 s, barras TX/RX, rename que sobrevive `peers-changed`.

### Docs
- **Auditoría de UI** (workflow: 4 dimensiones + verificación adversarial + consolidación, 30 agentes) → **18 hallazgos verificados** contra el código.
- **`docs/SPEC-ui-polish.md` (READY)** — spec delta de pulido de UI: 6 tareas (recortes de info; drag&drop→FILE y "0 B"; UX chicos; rediseño de la cola; Config colapsable con `<details>`; contraste con preview), bloque **NO SE TOCA** (motor de transferencia, render por diff, escaping, CSP+cert pinning, backend, datos), 9 criterios verificables. Decisiones del dueño: D1 auto-selección al primer peer **visible** según el filtro; D2 contraste **con preview** (solo si aprueba); D3 los 2 hallazgos que tocan el render por diff (conteo de peers repetido, navegar la lista con teclado) quedan **diferidos** a un spec aparte.

## 2026-07-14 — Fase 3 (Rust compartido + frontend): seguridad (IMPLEMENTADA + review aplicado; **release v0.16.0** + verificación física del core OK)

Spec archivado en `docs/archive/phase-3-security.md`. Un commit por Tarea (ver `git log`). **Verificación por máquina: OK** — `cargo check`/`clippy` (sin warnings nuevos vs baseline; la única nueva, `prepare_upload` a 8 args, suprimida con `#[allow]`) / `cargo build` (debug) linkea / `node --check` main.js+pre.js OK. **4 harness aislados verdes** (los tests van `#[cfg(not(windows))]` por el bug de carga del binario de test): `safe_join` (reservados/ADS/dots), `extract_sha256` (marker/case/ausente), verifier de pinning (match/mismatch/case-insensitive/TOFU), y **handshake TLS real e2e** (rustls+rcgen): peer con clave real → OK; atacante con cert copiado + clave distinta → FAIL `BadSignature`; TOFU → OK. **Decisiones del dueño**: 3.4 `/text` queda ABIERTO (solo toast, no toca portapapeles ni disco); 3.6 updater ABORTA si no hay hash. **Publicado como release final v0.16.0** (bump `5bb57e4`) con el `.exe` (9.8 MB); GitHub calcula el `digest` per-asset que el updater nuevo verifica. **Verificación física del usuario (2026-07-14): OK en el core** — auto-update v0.15.0→v0.16.0 en las 2 PCs + transferencias bidireccionales funcionando (pinning no rompe el uso diario, CSP no rompe la app). Pendiente solo lo opcional (ataque simulado —ya probado por máquina con el harness de handshake→`BadSignature`—, bulk de ~50 archivos, F12 explícito; ver TODO 🟢).

### Added
- **Cert pinning real (Tarea 3.1)** — `PinnedFingerprintVerifier` (rustls `ServerCertVerifier`) hashea el cert end-entity DER (SHA-256) y exige que matchee la fingerprint esperada; **valida además la firma del handshake** (delega a `rustls::crypto::verify_tls12/13_signature` con el provider ring) para probar posesión de la clave privada. `client_for(expected_fp)` cachea un `reqwest::Client` **por fingerprint** (`Mutex<HashMap>`, clone Arc barato) → el pooling que evita LocalSend #1657 sigue vivo; el lock se suelta antes de todo `await`. `fetch_info_pinned` (poller) vs `fetch_info` TOFU (discovery/pairing/self-ping). `ring` instalado como crypto provider al arranque.
- **CSP estricta (Tarea 3.2)** en `tauri.conf.json` (`default-src 'self'`; `script-src 'self'` sin `unsafe-inline`; `object-src 'none'`; `base-uri 'self'`; `frame-ancestors 'none'`; `img-src` con `data:`; `connect-src` con el IPC). 4 fuentes (Orbitron/Audiowide/Share Tech Mono/JetBrains Mono) **auto-hospedadas** en `src/fonts/*.woff2` (subset latin; Orbitron y JetBrains Mono son variables → 1 archivo/familia) + `@font-face`. Nuevo `src/pre.js` (ex `<script>` inline del `<head>`). Helper `iconSvg()` (lookup con `hasOwnProperty`).
- **Endurecimiento de `safe_join` (Tarea 3.7)** — `is_safe_component` rechaza `CON/PRN/AUX/NUL/COM1-9/LPT1-9` (con o sin extensión), `:` (ADS), chars ilegales NTFS, bytes de control y `.`/espacio final; corre para `rel_path` y `name`.
- **`DefaultBodyLimit` por-ruta (Tarea 3.5)** — `/clipboard/image` 48 MiB, `/prepare-upload` 8 MiB; el resto mantiene el default (2 MiB); `/upload` sin límite chico (streamea).
- **Integridad del updater (Tarea 3.6)** — `UpdateInfo.download_sha256` + verificación SHA-256 del binario **antes** de escribir el staged `.exe`/`.bat`/`.apk`; aborta si no matchea o no hay hash. `extract_sha256` parsea `sha256:<64 hex>` del body.

### Changed
- **`/text` documentado como endpoint abierto (Tarea 3.4)** — decisión del dueño: cualquier peer del LAN puede mandar texto (solo dispara un toast; no toca portapapeles ni disco, a diferencia de `/clipboard`). Sin gate nuevo; documentado en el handler.
- **Firmas de `http_client`**: `post_text`/`prepare_upload`/`upload_file`/`cancel_upload` reciben `expected_fp`; `post_clipboard`/`_image` pin-ean a la fingerprint del **receptor** (antes se descartaba). El poller de clipboard arrastra la fp del receptor.
- **Updater lee el hash del `digest` per-asset de GitHub** (fix del review) — `check_for_update` toma `assets[].digest` (`sha256:<hex>`) del asset seleccionado, atado a la plataforma; `extract_sha256(body)` queda de fallback. Sin esto, un release unificado (exe+apk) rompía Android y ningún release actual (que no publica hash en el body) podía actualizar.

### Removed
- **`danger_accept_invalid_certs(true)`** de `http_client.rs` (Tarea 3.1): el cliente ya no acepta cualquier cert.
- **Probe `/info` pre-envío spoofeable** en `send_text`/`send_files`: corría en otro socket que el payload (ventana MITM/TOCTOU); la pin del transporte lo cubre.
- **`<link>` a Google Fonts** y el `<script>` inline del `<head>` en `index.html`; 3 `onclick` inline migrados a `addEventListener` (`add-peer-cancel` no tenía listener y quedaba muerto sin el onclick → cableado).

### Fixed
- **MITM con cert copiado (review, CRÍTICO)** — el verifier devolvía `Ok(assertion())` en `verify_tls12/13_signature` sin chequear la firma, así que pin-eaba el hash de bytes públicos sin probar posesión de la clave. Un atacante copiaba el cert de un peer emparejado y hacía MITM. Ahora la firma se valida contra la clave pública del cert presentado.
- **XSS por datos de peer en `innerHTML` (Tarea 3.3 + review)** — thumbnail entrante (createElement + validación `data:image/`), `senderIp`/`senderPort` (textContent), branch de error del QR (textContent), y `iconType` de la TXT de mDNS (ya no se interpola en `data-icon` del `innerHTML` de `buildPeerItem`; `dataset` no parsea HTML). Refutados por el review (no aplicados): bypass Unicode COM¹/²/³ de `safe_join` (con prefijo de dir Windows no redirige al dispositivo, probado en la máquina real); framing "updater 100% roto" (abortar-sin-hash es decisión documentada).

## 2026-07-13 — Fase 2 Windows: correctness y seguridad de datos (IMPLEMENTADA + review aplicado; falta verificación física)

Spec archivado en `docs/archive/phase-2-correctness.md`. Un commit por Tarea (2.2 dividida en sub-bugs; ver `git log`). **Verificación por máquina: OK** — `cargo check`/`clippy` (13 warnings = baseline, 0 nuevos) / `cargo test --lib` 7/7 / `node --check` OK. **Round-trip sobre los 6 JSON reales del usuario: OK** (harness aislado sin Tauri, `include!` del `json_store.rs` vivo — los 6 stores cargan→guardan→cargan idéntico; `settings` pierde solo el campo vestigial `registerSendTo`, comportamiento pre-existente). **`.bat` de update probado a mano** (fallo→marcador tras 10 tries; feliz→swap+limpia). **Falta verificación física del usuario** (datos reales + UI, ver HANDOFF).

### Added
- **Módulo `json_store.rs`** — `JsonStore<T>` genérico: escritura atómica (`<file>.tmp` + `fs::rename` = replace atómico en Windows) y **backup-on-corrupt** (ante parseo fallido copia el crudo a `<file>.corrupt`, loguea `ERR` y cae a default — nunca más un `unwrap_or_default()` silencioso). `update()` mantiene el `Mutex` a través de serialize+persist (I/O sync, no viola la regla del `.await`), serializando writes concurrentes sobre el mismo store. 3 tests unitarios (round-trip, backup-on-corrupt, sin `.tmp` residual) gateados `#[cfg(all(test, not(windows)))]` (ver Fixed/harness).
- **Barra de progreso RX** propia (`#rx-progress-block`, `setRxProgress`) separada de la TX, keyeada por `sessionId` (`state.activeReceive`). Superficie DOM `#incoming-toast` para el texto entrante, separada del toast de ACK.
- **Comando `take_update_failure`** (pull) que el frontend consume en boot para mostrar un update que falló, sin depender de un emit temprano.

### Changed
- **Los 6 stores** (`preferences`, `settings`, `aliases`, `icon_overrides`, `manual_peers`, `clipboard_sync`) delegan el I/O en `JsonStore` conservando **firmas públicas, nombres de archivo y formato en disco** idénticos. `settings` usa `load_with_default` (no tiene `Default`); `loaded_from_corrupt()` preservado; `clipboard_sync` mantiene `last_synced_hash` y `hash_text/bytes` fuera del store.
- **`setStatus(msg, {priority, ttl, force})`**: un info (mensaje de grilla ~5 s) ya no pisa un `warn`/`err` vigente hasta su TTL; una acción consciente (`selectPeer`) o una confirmación de éxito pasan `{force}`.
- **Zombie-killer** (`windows_integration.rs`): mata por dueño del puerto `53319` **solo si el owner es uno de nuestros procesos** (chequeo por ambos nombres de deploy: `millennium-clipboard` build y `Millennium Clipboard` release renombrado), más por nombre; excluye el PID propio; **skip cuando `MILLENNIUM_INSTANCE` está seteada** (dev double-launch). Sin wildcards.
- **`.bat` de update** (`updater.rs`): reintenta el `move` en loop (hasta 10×, ~1 s c/u) y si falla deja `<temp>\millennium-update-failed.txt`; en éxito borra un marcador rancio.

### Fixed
- **Pérdida silenciosa de datos** (Tarea 2.1): un corte a mitad de escritura ya no trunca un store (rename atómico); un byte corrupto ya no resetea favoritos/peers sin dejar rastro (`.corrupt` + log).
- **UI que se pisaba** (Tarea 2.2): `peer.status` desconocido ya no congela la grilla (normalizado a `offline` + try/catch por fila; se sacó la clase `reaching` inexistente y se limpia `away`); el peer seleccionado que desaparece muestra `TARGET LOST` y **no se re-apunta solo** (no se manda al peer equivocado); el texto entrante ya no lo destruye un ACK; TX y RX ya no pelean por la misma barra; un rename inline sobrevive un `peers-changed`.
- **Zombie-killer inútil en release** (Tarea 2.3): el bug apuntaba solo a `millennium-clipboard` y no liberaba el puerto del zombie real.
- **Update que fallaba en silencio** (Tarea 2.4): un `move` único que fallaba dejaba al usuario en la versión vieja creyendo que actualizó; ahora reintenta y, si no puede, avisa al próximo arranque.
- **Review adversarial (5 dim × 2 escépticos; 3 confirmados + 1 endurecimiento aplicados; 3 refutados)**: el TTL de `setStatus` suprimía la confirmación tras una acción del usuario (→ `{force}`); el zombie-killer mataba al dueño del puerto sin chequear propiedad (→ chequeo por nombre); el aviso de update fallido se perdía si el webview no estaba listo (→ modelo pull). Refutados: "mata instancia sana en doble-launch" (pre-existente, no lo introdujo el diff), "auto-select solo en initial rompe cold-boot" (es lo que pide el spec), race del `.tmp` (comandos sync serializados; igual endurecido).
- **Harness de test de Tauri en Windows**: agregar cualquier test al crate hace que el linker MSVC deje de podar el stack GUI de tao/wry en el binario de test del lib, que entonces importa símbolos comctl32-v6 (`TaskDialogIndirect`) sin el manifest que embebe `tauri-build` → `STATUS_ENTRYPOINT_NOT_FOUND` al cargar. Los tests de `json_store` se gatearon `not(windows)` y se verificaron en un harness aislado. Anotado en TODO.

## 2026-07-13 — Fase 1 Windows: consolidar discovery / fin del parpadeo (COMPLETADA Y VERIFICADA — core)

Spec archivado en `docs/archive/phase-1-discovery.md`. Un commit por Tarea (ver `git log`). **Verificación por máquina: OK** (`cargo check`/`clippy` sin warnings nuevos / `test` 7/7 / `build` linkea / `node --check`). **Verificación física con 2 dispositivos (2026-07-13): OK en lo core** — las 2 PCs se ven, el peer no parpadea, CPU ~0 en reposo, el reaper marca offline en ~15 s, y las transferencias andan en ambos sentidos (build release desplegado en las 2 PCs). Queda sin probar físicamente lo opcional (roaming / QR tras cambio de red): verificado por máquina, no físico.

### Changed
- **Política única de reconciliación del `PeerMap`** (Tarea 1.1): `PeerRecord` gana `confirmed: bool`. La ruta (ip/port) de un peer se considera confirmada cuando la probó una fuente real (src IP de un datagrama UDP, o un probe TCP a `/info`). mDNS ya **no pisa** una ruta confirmada — solo refresca metadata (alias/hex/icono) o inserta peers nuevos (fn pura `reconcile_mdns`). El datagrama UDP es autoritativo: corrige ip/port y ahora **emite `peers-changed`** en la corrección (antes solo logueaba `IP DISAGREEMENT`). Es el root cause del flap asimétrico mDNS-vs-UDP.
- **Poller reescrito en dos tasks** (Tareas 1.2/1.3/1.4/1.5): se elimina el barrido de probe TCP a *todos* los peers cada 6 s (el gasto principal en reposo). (A) **reaper** cada 2 s marca offline al peer cuyo `last_seen` supera `PEER_TTL` = 3× `BROADCAST_INTERVAL_SECS` (15 s). (B) **probe scheduler** cada 2 s solo sondea a quien UDP no mantiene fresco: manual/favoritos nunca oídos (backoff exponencial 6 s→5 min por peer) y peers vivos que se ponen stale. El contador `u8` de fallos se reemplaza por maps `backoff`/`probe_at` purgados por tick (imposible desbordar / sin fuga). Ambos intervalos con `MissedTickBehavior::Skip`.
- **Selección de `local_ip`** (Tarea 1.7): en vez de la IP de la routing table (a menudo una NIC virtual WSL/Hyper-V/VPN), se enumeran las interfaces con `list_afinet_netifas()` y se elige la primera IPv4 privada, no-loopback, no-virtual (fn pura `pick_local_ipv4`). Watcher cada 30 s que, ante un cambio de red, re-habilita la interfaz mDNS y re-anuncia. Divergencia con el spec: se reusó el crate `local-ip-address` ya presente en vez de sumar `if-addrs` — mismo fix, cero dep nueva.

### Removed
- **Gate `/24` cableado** (Tarea 1.3): el `retain` que borraba del cache a los peers de otro `/24` (falsos "unreachable" en LANs `/16`, `/23`, …) y la fn `subnet_prefix_24`. La alcanzabilidad la decide el probe, no una heurística de octetos.
- **Broadcast dirigido** `derive_subnet_broadcast` (Tarea 1.3): asumía `/24` y armaba `x.y.z.255`. Queda solo el *limited broadcast* `255.255.255.255`, que llega a todo el segmento sin conocer la máscara.
- **`browse()` por tick** (Tarea 1.2): el poller rebrowseaba mDNS en cada iteración (arriesgaba matar el listener). Queda el browse inicial de `start()` + `rebrowse()` bajo demanda (comando `rescan_peers`).

### Added
- 6 tests unitarios Rust nuevos: `reconcile_mdns` (mDNS no pisa ruta confirmada; sí actualiza no-confirmada; metadata siempre refresca) y `pick_local_ipv4` (física sobre virtual/loopback; salta APIPA/pública; None si solo hay virtual/loopback).

### Fixed (review adversarial multi-agente — 5 dimensiones × 2 escépticos; 9 hallazgos, 0 refutados, 5 confirmados + 3 nits, todos aplicados)
- **Reap de peer vivo (medio)**: `join_all` retenía el refresh de `last_seen` de un peer sano hasta que terminaba el probe más lento (timeout 5 s); un peer muerto co-agendado podía empujarlo por encima del `PEER_TTL` y el reaper lo mataba vivo → volvía el parpadeo para peers probe-only. Ahora `FuturesUnordered`: cada probe se procesa apenas responde.
- **Rescan sin fuerza (medio)**: `rescan_peers` solo hacía mDNS rebrowse; un favorito solo-TCP en backoff (hasta 5 min) no se re-sondeaba. Ahora `DiscoveryState.wake_probes()` (tokio `Notify`) fuerza un probe inmediato con backoff limpio.
- **IP vieja en el QR (bajo/medio ×2)**: el watcher mutaba una **copia** de `Identity`; tras un roam, el QR de emparejamiento y `get_local_info` seguían mostrando la IP de arranque. Ahora la IP vive en `DiscoveryState.current_ip` (compartida) que el watcher actualiza y que QR/`get_local_info` leen.
- **Log de fallos de probe (bajo)**: se restauró — se loguea el primer fallo/timeout de cada racha con ip/port/motivo, sin spam por tick.
- **Nits**: fullnames huérfano al dropear por DRIFT (ahora se reconcilia contra el live set); `compute_local_ip()` (syscall bloqueante) del watcher movido a `spawn_blocking`; quitado el `#[allow(dead_code)]` de `last_seen` (ya lo leen reaper y scheduler).

## 2026-07-13 — Fase 0 Windows: parar la hemorragia de CPU/RAM (COMPLETADA Y VERIFICADA)

Spec archivado en `docs/archive/phase-0-stop-the-bleed.md`. Un commit por Tarea (ver `git log`). **Verificación física por el usuario: OK** — CPU casi nulo en reposo con imagen en el portapapeles, sync de clipboard E2E entre 2 máquinas, FX/logs. Autostart verificado end-to-end (copia al escritorio → cierre → arranque desde ahí → el heal reescribe la entrada `Run` al exe actual → arranque limpio sin crash; Windows resuelve la ruta pese a no llevar comillas). Descubierto de paso: la entrada `Run` del plugin va sin comillas (*unquoted path*, CWE-428) — anotado para la Fase 3.

### Fixed
- **Clipboard poller** (Tarea 0.1): thread dedicado con UN solo handle de arboard; no toca el portapapeles sin peers con sync habilitado; en Windows gatea por `GetClipboardSequenceNumber` (FFI directo a user32, sin dep nueva — divergencia menor con el spec que pedía la crate `windows`); hashea el RGBA crudo antes de pagar el encode PNG+base64; intervalo 500 ms → 1200 ms. **Bug fix**: texto >1 MB se descarta con log en vez de caer al branch de imagen. El anti-eco sigue clavado al hash del PNG (compatible con el receptor).
- **Tormenta de logs** (Tarea 0.2): el frontend guarda un ring de 2000 líneas (antes: string infinito = fuga de RAM); el backend emite `log-line` por IPC solo con el panel abierto (comando nuevo `set_log_panel_open`); el buffer de 5000 y el archivo `runtime.log` se escriben siempre. El poller de discovery deduplica `skipping … different /24` (una vez por peer/IP) y `probe failed/TIMEOUT` (los 3 que importan, después cada 10).
- **Overflow del contador de failures** en discovery (u8 con `+= 1` infinito → desbordaba a los ~25 min de un favorito muerto; panic en builds debug). Ahora `saturating_add`.
- **Autostart stale** (Tarea 0.5): al arrancar, si `start_with_windows` está activo, la entrada `HKCU\...\Run` se reescribe apuntando al `.exe` actual (sanea el path muerto de v0.8.1); con la pref apagada se borra la entrada fantasma.
- **`get_settings` devolvía 2 de 5 campos** (extra, fuera de spec): `startWithWindows` / `notificationsEnabled` / `closeToTray` no llegaban al frontend y los toggles de Settings abrían siempre en su default.

### Changed
- **FX compositor-only** (Tarea 0.3): el grid del horizonte anima `transform: translateY` en un `::before` (antes `background-position` = repaint continuo); `.card` sin `backdrop-filter`; `.noise` sin `mix-blend-mode`; toda animación decorativa en loop vive tras `prefers-reduced-motion`; `html.fx-paused` congela todo con la ventana oculta; toggle **VISUAL FX** nuevo en Settings (persistido en localStorage) con `html.fx-off`; el typewriter del placeholder se frena en background.
- **`[profile.release]`** (Tarea 0.4): `lto` + `codegen-units=1` + `strip` + `panic="abort"` + `opt-level="s"`. El panic hook sigue escribiendo `crash.log`.

### Added
- Primer test unitario Rust del proyecto (`is_syncable_text`, gate de tamaño del clipboard de texto) — `cargo test` verde.

### Review adversarial (multi-agente sobre el diff completo de la fase)
Un workflow de 5 dimensiones × verificación por 2 escépticos encontró 9 defectos reales introducidos por la fase (3 hallazgos más fueron refutados con evidencia). Todos corregidos:
- **Privacidad (major)**: al activar clipboard-sync se retransmitía contenido copiado *antes* del opt-in. El poller ahora prima `last_seq` en el arranque y marca los cambios como vistos aunque no haya peers → solo se sincroniza lo copiado con sync ya activo.
- **Typewriter (major)**: arrancaban dos cadenas de placeholder en el boot (una quedaba corriendo en la bandeja); y el freno al ocultar la ventana fallaba ~38% de las veces por un timer sin rastrear. Ahora hay una sola cadena, cancelable, que se detiene de verdad en background.
- **Robustez del poller (minor)**: un lock transitorio del portapapeles (otra app) hacía perder el cambio para siempre; ahora se distingue "lock transitorio" (reintenta) de "ya procesado".
- **Log (minor ×2)**: COPY/EXPORT traían solo 2000 líneas aunque el contador anuncia hasta 5000 (ahora piden el buffer completo al backend); y una recarga del webview con el panel abierto dejaba el emit IPC prendido (ahora el boot resetea el flag).
- **Autostart (minor)**: con `settings.json` corrupto, el heal borraba la entrada `Run` del usuario; ahora saltea si el settings no parseó.
- **Scanline (minor)**: quedaba como línea fija con `prefers-reduced-motion`; ahora se oculta.
- **`panic="abort"` (minor)**: documentado el trade-off (tumba el proceso entero ante cualquier panic) en `Cargo.toml`; se mantiene porque lo pide el spec.

## 2026-07-13 — montaje del sistema de trabajo

### Added
- Sistema de trabajo del playbook: `CLAUDE.md`, skills `/inicio` `/cierre` `/smoke`, hooks (SessionStart + check-code), `.claude/settings.json`.
- Documentación operativa: `docs/SESSION_HANDOFF.md`, `docs/TODO.md` (sembrado con las fases del spec de remediación), `docs/CHANGELOG.md`.

### Changed
- `.gitignore`: se ignora `.claude/settings.local.json` (el resto de `.claude/` — skills, hooks, settings — se versiona).
