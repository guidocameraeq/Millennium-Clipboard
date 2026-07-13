# CHANGELOG — Millennium Clipboard

> Historia permanente. `/cierre` agrega una entrada AL TOPE en cada sesión. Orden descendente estricto, sin excepciones. Nada de versiones duplicadas en otros docs.

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
