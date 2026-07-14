# CHANGELOG — Millennium Clipboard

> Historia permanente. `/cierre` agrega una entrada AL TOPE en cada sesión. Orden descendente estricto, sin excepciones. Nada de versiones duplicadas en otros docs.

## 2026-07-14 — Fase 3 (Rust compartido + frontend): seguridad (IMPLEMENTADA + review aplicado; falta verificación física)

Spec archivado en `docs/archive/phase-3-security.md`. Un commit por Tarea (ver `git log`). **Verificación por máquina: OK** — `cargo check`/`clippy` (sin warnings nuevos vs baseline; la única nueva, `prepare_upload` a 8 args, suprimida con `#[allow]`) / `cargo build` (debug) linkea / `node --check` main.js+pre.js OK. **4 harness aislados verdes** (los tests van `#[cfg(not(windows))]` por el bug de carga del binario de test): `safe_join` (reservados/ADS/dots), `extract_sha256` (marker/case/ausente), verifier de pinning (match/mismatch/case-insensitive/TOFU), y **handshake TLS real e2e** (rustls+rcgen): peer con clave real → OK; atacante con cert copiado + clave distinta → FAIL `BadSignature`; TOFU → OK. **Decisiones del dueño**: 3.4 `/text` queda ABIERTO (solo toast, no toca portapapeles ni disco); 3.6 updater ABORTA si no hay hash. **Falta verificación física del usuario** (pinning con 2 instancias/PCs, CSP sin violaciones en F12, peer emparejado sigue transfiriendo — ver HANDOFF).

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
