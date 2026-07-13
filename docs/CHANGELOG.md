# CHANGELOG — Millennium Clipboard

> Historia permanente. `/cierre` agrega una entrada AL TOPE en cada sesión. Orden descendente estricto, sin excepciones. Nada de versiones duplicadas en otros docs.

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
