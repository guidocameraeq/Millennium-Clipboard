# Millennium Clipboard — Windows · Fase 0: Parar la hemorragia

> **ESTADO: COMPLETADA Y VERIFICADA 2026-07-13.** Código verde (cargo check/clippy/test, .exe release 9.8 MB) + verificación física por el usuario: CPU casi nulo en reposo, sync E2E OK, FX/logs OK. Autostart verificado end-to-end (heal reescribe la entrada al exe actual; Windows la resuelve pese a no llevar comillas — ver nota abajo). Commits: 88fd306 → b6479c3 + review adversarial (9 fixes). Archivada.
>
> **Pendiente derivado (no bloqueante, va a Fase 3 seguridad):** la entrada `HKCU\...\Run` que escribe `tauri-plugin-autostart` NO lleva comillas; con rutas que tienen espacios (ej. `OneDrive\Desktop eQ\Millennium Clipboard.exe`) Windows la resuelve por su heurística de búsqueda, pero es un *unquoted path* (CWE-428) frágil/inseguro. Fix futuro: reescribir la entrada con comillas.

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Windows (0.1, 0.5 tocan código `not(target_os = "android")` / `desktop`; 0.2–0.3 tocan frontend compartido; 0.4 es build) · **Prerrequisitos:** ninguno (esta es la primera fase) · **Esfuerzo:** ~1 día · **Riesgo:** med

## Objetivo
Eliminar las tres causas verificadas de consumo absurdo de CPU/RAM en escritorio (el poller de clipboard que codifica PNG/base64 cada 500 ms, la tormenta de logs que crece sin límite en el frontend y se emite siempre, y los efectos visuales que fuerzan repaint/composite continuo), y sumar dos mejoras rápidas de build (perfil `release` optimizado y reparación del autostart que apunta a un `.exe` v0.8.1 inexistente). Después de esta fase la app en reposo debe consumir CPU casi nula y un binario mucho más chico.

## Definición de "hecho"
- [ ] Con la app abierta y sin peers con clipboard-sync habilitado, el poller NO llama a `arboard::Clipboard::new()` ni codifica PNG/base64 en cada tick (verificable con un log de una sola línea y por CPU en el Task Manager ~0%).
- [ ] El intervalo del poller pasó de 500 ms a 1000–1500 ms y reutiliza UN solo handle `arboard::Clipboard`.
- [ ] El texto de >1 MB ya NO cae al branch de imagen: queda descartado explícitamente en lugar de ir a `get_image()`.
- [ ] El `logBuffer` del frontend es un ring array acotado (máx. 2000 líneas), no un `String` que crece sin fin.
- [ ] El backend solo emite `log-line` cuando el panel de log está abierto (comando que el frontend togglea); las líneas repetidas de `[poll] probe failed …` / `[poll] skipping … different /24` se deduplican.
- [ ] Las animaciones decorativas están envueltas en `@media (prefers-reduced-motion: no-preference)`, se pausan con una clase en `document.hidden`/`visibilitychange`, `.card` ya no tiene `backdrop-filter`, `.noise` ya no tiene `mix-blend-mode`, el grid usa `transform: translateY` y existe un toggle de FX.
- [ ] El typewriter del placeholder se detiene cuando la ventana está en background.
- [ ] `Cargo.toml` tiene `[profile.release]` con `lto`, `codegen-units=1`, `strip`, `panic="abort"`, `opt-level="s"`; el `.exe` de release baja de ~25 MB a ~8–12 MB y el panic hook sigue escribiendo `crash.log`.
- [ ] Al arrancar, si `start_with_windows` está activo, la entrada de autostart se re-registra apuntando al `.exe` ACTUAL (no al path stale de v0.8.1).

## Tareas

### Tarea 0.1 — Poller de clipboard: gate barato antes de codificar

**Problema.** `spawn_clipboard_poller` corre cada 500 ms y en CADA tick: abre un `arboard::Clipboard::new()` nuevo, lee texto, y si no hay texto lee la imagen, la re-encodea a PNG en memoria y la pasa a base64 — TODO antes de siquiera mirar si hay algún peer con sync habilitado o si el contenido cambió. Con una imagen en el portapapeles esto quema CPU sin parar. Además, el texto de >1 MB no matchea `text.len() <= 1_000_000`, así que cae al branch de imagen (bug de "text falling through to image").

**Archivo(s).** `src-tauri/src/lib.rs:718-864` (la variante `#[cfg(not(target_os = "android"))]` de `spawn_clipboard_poller`).

**Estado actual.**
```rust
tauri::async_runtime::spawn(async move {
    let mut last_text: Option<String> = None;
    let mut last_image_hash: Option<String> = None;
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
    tick.tick().await;
    loop {
        tick.tick().await;

        // Pull whatever the OS clipboard currently holds on a blocking
        // worker (arboard reads are blocking).
        let snap: Option<ClipSnapshot> = tokio::task::spawn_blocking(|| {
            let mut cb = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(_) => return None,
            };
            if let Ok(text) = cb.get_text() {
                if !text.is_empty() && text.len() <= 1_000_000 {
                    return Some(ClipSnapshot::Text(text));
                }
            }
            if let Ok(img) = cb.get_image() {
                // ... width/height checks, RgbaImage::from_raw,
                //     PNG encode, hash_bytes, base64 encode ...
                return Some(ClipSnapshot::Image { png_base64, hash });
            }
            None
        })
        .await
        .ok()
        .flatten();
        // ... diff-and-debounce, targets, send ...
```

**Cambio.** Reestructurar el loop con tres compuertas baratas ANTES de cualquier encode caro, en este orden:

1. **Gate por peers.** Si `store.enabled_snapshot().is_empty()`, hacer `continue` sin tocar el portapapeles. `enabled_snapshot()` (ver `clipboard_sync.rs:75`) solo clona un `HashSet<String>` bajo mutex — es barato.
2. **Detección de cambio barata para imagen** vía `GetClipboardSequenceNumber` (Windows) antes de leer/encodear. Si la secuencia no cambió desde el último tick, `continue`. `GetClipboardSequenceNumber` es un `u32` global que Windows incrementa cada vez que cambia el portapapeles; para leerlo agregar la dependencia `windows` con la feature `Win32_System_DataExchange` (ver **Cuidado con**). Fallback portable: si no se quiere la dep, hashear los bytes RGBA crudos (`img.bytes`) ANTES de encodear PNG y comparar contra `last_image_hash`.
3. **Un solo handle** `arboard::Clipboard` reusado a través de los ticks en vez de `::new()` por tick.
4. **Intervalo** subido a 1000–1500 ms.
5. **Bug de texto >1 MB.** Si hay texto pero `text.len() > 1_000_000`, descartarlo explícitamente (loggear y `continue`) en vez de dejar que caiga a `get_image()`.

El handle de `arboard` no es `Send`-friendly a través de `.await` de tokio, así que mantener un `arboard::Clipboard` de larga vida dentro de un único `spawn_blocking` de larga duración es frágil. La forma más simple y robusta: sacar el `arboard::Clipboard::new()` fuera del loop hacia un thread dedicado con `std::thread::spawn` que se comunica por canal, O — más chico — reusar el handle sólo dentro de `spawn_blocking` moviéndolo dentro y devolviéndolo. Dado el objetivo "smallest change", usar un `std::thread` dedicado con un `tokio::sync::mpsc` de vuelta. Sketch del algoritmo:

```rust
#[cfg(not(target_os = "android"))]
fn spawn_clipboard_poller(
    peers: discovery::PeerMap,
    store: Arc<clipboard_sync::ClipboardSyncStore>,
    my_alias: String,
    my_fingerprint: String,
) {
    // Un canal por el que el thread bloqueante entrega snapshots ya diffeados.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ClipSnapshot>(4);

    // ---- Thread dedicado: dueño de UN arboard::Clipboard para toda su vida ----
    std::thread::spawn({
        let store = store.clone();
        move || {
            let mut cb = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    runtime_log::err(format!("[clipboard] arboard init failed: {e}"));
                    return;
                }
            };
            let mut last_text: Option<String> = None;
            let mut last_image_hash: Option<String> = None;
            #[cfg(target_os = "windows")]
            let mut last_seq: u32 = 0;

            loop {
                std::thread::sleep(std::time::Duration::from_millis(1200));

                // GATE 1: ¿algún peer con sync? Si no, ni tocamos el portapapeles.
                if store.enabled_snapshot().is_empty() {
                    continue;
                }

                // GATE 2 (Windows): ¿cambió el portapapeles desde el último tick?
                #[cfg(target_os = "windows")]
                {
                    let seq = unsafe {
                        windows::Win32::System::DataExchange::GetClipboardSequenceNumber()
                    };
                    if seq == last_seq {
                        continue; // nada cambió — cero encode, cero read pesado
                    }
                    last_seq = seq;
                }

                // Recién ahora leemos. Texto primero.
                if let Ok(text) = cb.get_text() {
                    if !text.is_empty() {
                        if text.len() > 1_000_000 {
                            // BUG FIX: texto grande NO cae a imagen; se descarta.
                            runtime_log::warn(format!(
                                "[clipboard] text too large ({} bytes) — skipped",
                                text.len()
                            ));
                            continue;
                        }
                        if last_text.as_deref() == Some(text.as_str()) {
                            continue;
                        }
                        last_text = Some(text.clone());
                        last_image_hash = None;
                        let hash = clipboard_sync::hash_text(&text);
                        if store.is_recent(&hash) {
                            continue;
                        }
                        store.note_synced(hash);
                        let _ = tx.blocking_send(ClipSnapshot::Text(text));
                        continue;
                    }
                }

                // Imagen: hasheamos RGBA CRUDO antes de encodear PNG/base64.
                if let Ok(img) = cb.get_image() {
                    let w = img.width as u32;
                    let h = img.height as u32;
                    if w == 0 || h == 0 || w > 8192 || h > 8192 {
                        continue;
                    }
                    let raw_hash = clipboard_sync::hash_bytes(&img.bytes);
                    if last_image_hash.as_deref() == Some(raw_hash.as_str()) {
                        continue; // misma imagen — NO encodeamos PNG
                    }
                    if store.is_recent(&raw_hash) {
                        continue;
                    }
                    // Recién acá pagamos el costo de PNG + base64.
                    let raw: Vec<u8> = img.bytes.into_owned();
                    let buf = match image::RgbaImage::from_raw(w, h, raw) {
                        Some(b) => b,
                        None => continue,
                    };
                    let mut png_bytes: Vec<u8> = Vec::with_capacity(256 * 1024);
                    {
                        let mut cursor = std::io::Cursor::new(&mut png_bytes);
                        if image::DynamicImage::ImageRgba8(buf)
                            .write_to(&mut cursor, image::ImageFormat::Png)
                            .is_err()
                        {
                            continue;
                        }
                    }
                    if png_bytes.len() > 32 * 1024 * 1024 {
                        continue;
                    }
                    last_image_hash = Some(raw_hash.clone());
                    last_text = None;
                    store.note_synced(raw_hash);
                    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
                    let png_base64 = B64.encode(&png_bytes);
                    let _ = tx.blocking_send(ClipSnapshot::Image {
                        png_base64,
                        hash: last_image_hash.clone().unwrap(),
                    });
                }
            }
        }
    });

    // ---- Tarea async: recibe snapshots ya diffeados y los reparte a peers ----
    tauri::async_runtime::spawn(async move {
        while let Some(snap) = rx.recv().await {
            let targets: Vec<(String, u16)> = {
                let enabled = store.enabled_snapshot();
                if enabled.is_empty() {
                    Vec::new()
                } else {
                    let p = peers.lock().unwrap();
                    enabled
                        .into_iter()
                        .filter(|fp| fp != &my_fingerprint)
                        .filter_map(|fp| p.get(&fp).map(|r| (r.ip.clone(), r.port)))
                        .collect()
                }
            };
            if targets.is_empty() {
                continue;
            }
            for (ip, port) in targets {
                // ... el mismo match &snap { Text => post_clipboard, Image => post_clipboard_image } de hoy ...
            }
        }
    });
}
```

Notá que ahora el `store.is_recent` / `note_synced` sobre imagen usan el hash del **RGBA crudo**, no del PNG. Eso cambia la semántica del anti-loop: el `ClipSnapshot::Image { hash, .. }` que se manda a peers ahora lleva el hash RGBA. Si el receptor compara contra el hash del PNG, hay que mantener consistencia — **KEEP** el `hash` que viaja a `post_clipboard_image` como el que ya se usa hoy si el lado receptor depende de él; revisá `http_client::post_clipboard_image` y el handler de `/clipboard` en `http_server.rs` antes de cambiar qué hash viaja. Si el receptor recalcula el hash del PNG de su lado, entonces mandá el hash del PNG (calculalo igual que hoy con `hash_bytes(&png_bytes)`) y usá el RGBA-hash SOLO como gate local de "no re-encodear".

**Por qué.** El costo dominante era encodear PNG + base64 de una imagen sin cambios, sin peers, cada 500 ms. Con el gate por peers vacío + el gate por secuencia/hash-crudo, en reposo el loop hace un `sleep` y un check `u32` — CPU ~0. Reusar un solo handle elimina el `arboard::Clipboard::new()` por tick (que en Windows abre/cierra el clipboard OLE cada vez).

**Cuidado con.**
- `arboard::Clipboard` NO es `Send` en algunas plataformas y no debe cruzar `.await`. Por eso vive en un `std::thread` propio, no en `spawn_blocking` dentro del loop async. NO lo muevas a una `tokio` task.
- La dep `windows`: agregar en `Cargo.toml` bajo `[target.'cfg(target_os = "windows")'.dependencies]` algo como `windows = { version = "0.58", features = ["Win32_System_DataExchange"] }`. La función exacta es `windows::Win32::System::DataExchange::GetClipboardSequenceNumber() -> u32` y es `unsafe`. Si preferís no sumar la dep, borrá el bloque `#[cfg(target_os = "windows")]` de secuencia y confiá solo en el hash RGBA crudo (GATE 2 se vuelve el hash) — sigue siendo mucho más barato que hoy porque el hash crudo evita el encode PNG.
- `tx.blocking_send` bloquea el thread si el canal está lleno (capacidad 4). Con un consumidor rápido eso está bien; no subas la capacidad a algo enorme.
- **KEEP** intacta la variante `#[cfg(target_os = "android")]` de `spawn_clipboard_poller` (lib.rs:705-716) — Android no usa arboard.
- **KEEP** la firma de `spawn_clipboard_poller` y su call site en `lib.rs:1390` sin cambios.

---

### Tarea 0.2 — Tormenta de logs: ring array en frontend + emit solo con panel abierto

**Problema.** (a) Frontend: `logBuffer` es un `String` al que se le concatena cada línea (`logBuffer += … + line`) para siempre — crece sin techo y `repaintLog()` hace `logBuffer.split('\n')` sobre todo el historial. (b) Backend: `runtime_log::push` hace `app.emit("log-line", …)` en CADA línea aunque el panel esté cerrado, y el poller de discovery emite `[poll] probe failed …` / `[poll] skipping … different /24` repetidamente.

**Archivo(s).** `src/main.js:1118-1169` (buffer + append/repaint) y `src/main.js:1816-1820` (listener `log-line`); `src-tauri/src/runtime_log.rs:95-112` (`push`); `src-tauri/src/discovery.rs:370-375` y `450-473` (líneas repetidas).

**Estado actual (frontend).**
```js
let logBuffer = '';
let logLines = 0;
// ...
function appendLogLine(line) {
  logBuffer += (logBuffer ? '\n' : '') + line;
  logLines += 1;
  if (logModal && !logModal.hidden) {
    const span = document.createElement('span');
    span.className = classifyLogLine(line);
    span.textContent = (logPane.childElementCount ? '\n' : '') + line;
    logPane.appendChild(span);
    // ...
  }
}
```

**Estado actual (backend `push`).**
```rust
pub fn push(level: &str, msg: String) {
    let line = format!("{} [{}] {}", iso_now(), level, msg);
    eprintln!("{}", line);
    let s = store();
    {
        let mut lines = s.lines.lock().unwrap();
        if lines.len() >= CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line.clone());
    }
    if let Some(app) = s.app.lock().unwrap().as_ref() {
        let _ = app.emit("log-line", &line);
    }
    if let Some(f) = s.file.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{}", line);
    }
}
```

**Cambio.**

**(a) Frontend — ring array acotado.** Reemplazar el `String` por un array con tope y unir bajo demanda:

```js
const LOG_CAP = 2000;
let logRing = [];   // ring de a lo sumo LOG_CAP líneas
let logLines = 0;   // contador total (para el "N lines")

function appendLogLine(line) {
  logRing.push(line);
  if (logRing.length > LOG_CAP) logRing.shift();
  logLines += 1;
  if (logModal && !logModal.hidden) {
    const span = document.createElement('span');
    span.className = classifyLogLine(line);
    span.textContent = (logPane.childElementCount ? '\n' : '') + line;
    logPane.appendChild(span);
    if (logLineCount) logLineCount.textContent = `${logLines} lines`;
    if (logAutoscroll && logAutoscroll.checked) {
      logPane.scrollTop = logPane.scrollHeight;
    }
  }
}
```

En `repaintLog()` y en el botón copy usar `logRing.join('\n')` en vez de `logBuffer`. En `openLogModal()`, tras `invoke('get_runtime_log')`, poblar el ring: `logRing = full ? full.split('\n').slice(-LOG_CAP) : [];`. Reemplazá TODAS las referencias a `logBuffer` (append, repaint, copy en `main.js:1180`) por el ring.

**(b) Backend — emit solo con panel abierto.** Agregar un flag global y dos comandos que el frontend togglea al abrir/cerrar el panel:

```rust
// runtime_log.rs
use std::sync::atomic::{AtomicBool, Ordering};
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

pub fn set_panel_open(open: bool) {
    PANEL_OPEN.store(open, Ordering::Relaxed);
}
```

En `push`, envolver el emit:
```rust
    if PANEL_OPEN.load(Ordering::Relaxed) {
        if let Some(app) = s.app.lock().unwrap().as_ref() {
            let _ = app.emit("log-line", &line);
        }
    }
```
El buffer en memoria (`s.lines`) y el archivo se siguen escribiendo SIEMPRE — solo se suprime el evento IPC. Agregar en `lib.rs` un comando `#[tauri::command] fn set_log_panel_open(open: bool) { runtime_log::set_panel_open(open); }` y registrarlo en `generate_handler![]` (junto a `get_runtime_log` en `lib.rs:1429`). En `main.js`, llamar `invoke('set_log_panel_open', { open: true })` dentro de `openLogModal()` y `{ open: false }` dentro de `closeLogModal()`. Como `openLogModal()` ya hace `get_runtime_log`, no se pierde historial mientras el panel estuvo cerrado.

**(c) Backend — deduplicar líneas repetidas.** Para `[poll] probe failed …` (`discovery.rs:453`) y `[poll] skipping … different /24` (`discovery.rs:370`), no re-loggear si el mensaje es idéntico al anterior para ese `fp`. La forma más chica: en `push` agregar un supresor de repetición consecutiva global:

```rust
// runtime_log.rs — dentro de RuntimeLog, un campo:
last_msg: Mutex<(String, u32)>, // (última línea sin timestamp, repeticiones)
```
En `push`, comparar `msg` (el texto sin `iso_now()`/level) contra el último; si es igual, incrementar el contador y NO empujar/emitir/escribir salvo cada N (p.ej. cada 10) con un sufijo `(x{n})`. Alternativa más localizada y menos invasiva: en `discovery.rs`, guardar en el `HashMap failures` el último mensaje emitido por `fp` y saltear el `runtime_log::warn` si no cambió el `(count, error-kind)`. Elegí la variante localizada en `discovery.rs` si querés evitar tocar `push` (menor riesgo de regresión en el logging general).

**Por qué.** El `String` sin tope es una fuga de RAM lenta con `repaintLog` cada vez más caro; el emit IPC por línea con el panel cerrado es puro overhead (serialización + cruce webview) que nadie ve. Deduplicar corta la avalancha de `probe failed` cuando un peer se cae.

**Cuidado con.**
- El `logLines` (contador total mostrado como "N lines") debe seguir contando el total real, no el tamaño del ring — no lo ates a `logRing.length`.
- **KEEP** el `CAPACITY = 5000` del buffer de Rust y el appender a archivo (`runtime_log.rs:22`, `109-111`) — son el canal de diagnóstico que el usuario pega; solo suprimimos el EVENTO, no el registro.
- Asegurate de setear `set_panel_open(false)` también si el modal se cierra por otra vía que no sea `closeLogModal()` (p.ej. tecla Escape). Buscá cualquier otro cierre del `logModal`.

---

### Tarea 0.3 — Efectos visuales: compositor-only, gated y pausables

**Problema.** Varios efectos fuerzan repaint/composite continuo aun con la ventana en background: `grid-flow` anima `background-position` (repaint del layer), `.card` tiene `backdrop-filter: blur(2px)` (recompone todo lo de atrás en cada frame), `.noise` usa `mix-blend-mode: overlay` (blending por frame de un layer full-screen), y varias animaciones (`scan-drop`, `dot-pulse`, `pulse-soft`, el typewriter JS) corren aunque nadie mire. En un WebView2 esto mantiene la GPU/CPU ocupadas 24/7.

**Archivo(s).** `src/styles.css:82-101` (`.horizon-floor` + `grid-flow`), `:116-138` (`.scanline`/`scan-drop`), `:141-149` (`.noise`), `:324` (`.card` `backdrop-filter`), `:205-212` (`.logo-mark` + `pulse-soft`), `:370-382` (`.card-pulse` + `dot-pulse`), `:722-726` (`.peer-status.online` `dot-pulse`); `src/index.html:38-44` (nodos `.bg-horizon`/`.scanline`/`.noise` y `.card`); `src/main.js:311-344` (typewriter). El keyframe `pulse-soft` está referenciado en `:211` pero su `@keyframes` no está en el rango leído — buscalo con Grep (`@keyframes pulse-soft`) antes de editar.

**Estado actual (grid).**
```css
.horizon-floor {
  /* ... background-image con las líneas del grid ... */
  background-size: 100% 100%, 64px 64px, 64px 64px;
  transform: rotateX(62deg);
  transform-origin: 50% 0%;
  animation: grid-flow 4s linear infinite;
}
@keyframes grid-flow {
  from { background-position: 0 0, 0 0, 0 0; }
  to   { background-position: 0 0, 0 64px, 0 0; }
}
```
**Estado actual (`.card`, `.noise`).**
```css
.card {
  /* ... */
  backdrop-filter: blur(2px);
}
.noise {
  /* ... */
  opacity: 0.04;
  mix-blend-mode: overlay;
  background-image: url("data:image/svg+xml;utf8,<svg ...feTurbulence.../>");
}
```

**Cambio.**

**(1) Grid vía `transform: translateY` (solo compositor).** Reimplementar el scroll del grid moviendo el elemento, no el `background-position`. Como el grid tiene `rotateX(62deg)` con `transform-origin: 50% 0%`, animar un wrapper interno o usar una animación que combine el `rotateX` fijo con un `translateY` que recorra 64px (el tamaño de celda) y loopee:

```css
.horizon-floor {
  /* mismas background-image / background-size, SIN animar background-position */
  transform: rotateX(62deg) translateY(0);
  transform-origin: 50% 0%;
  will-change: transform;
  animation: grid-scroll 4s linear infinite;
}
@keyframes grid-scroll {
  from { transform: rotateX(62deg) translateY(0); }
  to   { transform: rotateX(62deg) translateY(64px); }
}
```
`translateY(64px)` = una celda, así el loop es continuo. `translate` de transform corre en el compositor sin repaint.

**(2) Sacar `backdrop-filter` de `.card`.** Borrar la línea `backdrop-filter: blur(2px);` (styles.css:324). Si se quiere conservar algo del look, subir levemente la opacidad del `--bg-card` de fondo — pero por defecto simplemente removerlo.

**(3) Sacar `mix-blend-mode` de `.noise`.** Borrar `mix-blend-mode: overlay;` (styles.css:147). Con `opacity: 0.04` el ruido sigue leyéndose como textura sutil sin el costo del blending por frame.

**(4) Envolver TODO lo decorativo en `@media (prefers-reduced-motion: no-preference)`.** Mover las declaraciones `animation: …` de `.horizon-floor`, `.scanline`, `.logo-mark`, `.card-pulse`, `.peer-status.online .status-dot` dentro de:
```css
@media (prefers-reduced-motion: no-preference) {
  .horizon-floor { animation: grid-scroll 4s linear infinite; }
  .scanline      { animation: scan-drop 7.5s linear infinite; }
  .logo-mark     { animation: pulse-soft 2.4s ease-in-out infinite; }
  .card-pulse    { animation: dot-pulse 1.4s ease-in-out infinite; }
  .peer-status.online .status-dot { animation: dot-pulse 1.8s ease-in-out infinite; }
}
```
Dejá las propiedades estáticas (color, box-shadow, transform base) fuera del media query; solo la `animation` va adentro.

**(5) Pausar con clase en `document.hidden`.** Agregar una regla que congele todas las animaciones cuando el `<html>` tiene la clase `fx-paused`:
```css
html.fx-paused *,
html.fx-paused *::before,
html.fx-paused *::after {
  animation-play-state: paused !important;
}
```
Y en `main.js`, cerca del bootstrap:
```js
function syncFxPaused() {
  document.documentElement.classList.toggle('fx-paused', document.hidden);
}
document.addEventListener('visibilitychange', syncFxPaused);
syncFxPaused();
```

**(6) Toggle de FX en Settings.** Agregar una clase `fx-off` en `<html>` que desactive por completo los efectos decorativos:
```css
html.fx-off .bg-horizon,
html.fx-off .scanline,
html.fx-off .noise { display: none; }
html.fx-off .horizon-floor,
html.fx-off .logo-mark,
html.fx-off .card-pulse,
html.fx-off .peer-status.online .status-dot { animation: none !important; }
```
En `main.js`, leer/escribir la preferencia en `localStorage` (`localStorage.getItem('fx') === 'off'`), togglear `document.documentElement.classList.toggle('fx-off', off)` al arranque, y agregar un checkbox en el modal de Settings que la persista. (No necesita ir al backend — es puramente visual y de arranque temprano; opcionalmente inicializarla en el `<script>` inline de `index.html:15-24` para evitar flash.)

**(7) Detener el typewriter en background.** El typewriter (`main.js:322-335`) se re-agenda con `setTimeout` indefinidamente. Colgar su pausa de `visibilitychange`:
```js
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    stopPh();
  } else if (!textarea.value && document.activeElement !== textarea) {
    phIdx = 0; typePh(placeholderLines[0]);
  }
});
```
`stopPh()` ya existe (`main.js:336-339`) y limpia `phTimer`. Con `fx-off` también conviene no arrancar el typewriter: envolvé el `typePh(placeholderLines[0])` inicial (`main.js:340`) en `if (!document.documentElement.classList.contains('fx-off')) { … }`.

**Por qué.** `background-position`, `backdrop-filter` y `mix-blend-mode` disparan repaint/composite full-screen por frame; `transform` no. Gatear por `prefers-reduced-motion`, `document.hidden` y un toggle explícito significa que, en reposo o minimizada, la app deja de repintar — que es donde se iba la CPU/GPU.

**Cuidado con.**
- El grid con `translateY(64px)` debe coincidir con el `64px` del `background-size` de las líneas, si no el loop "salta". Mantené ambos en 64px.
- No rompas el layout de `.card`: al sacar `backdrop-filter` el fondo puede quedar más "plano" — es aceptable, no compenses con otro efecto caro.
- `animation-play-state: paused` con `!important` en `*` es contundente: verificá que no haya animaciones funcionales (no decorativas) que dependan de correr en background. En esta app las animaciones son todas decorativas, así que es seguro.
- **KEEP** los `@keyframes` existentes (`scan-drop`, `dot-pulse`, `pulse-soft`); solo cambia dónde/ cómo se aplican y se agrega `grid-scroll` en reemplazo de `grid-flow`.

---

### Tarea 0.4 — Build: perfil release optimizado

**Problema.** No hay `[profile.release]` en `Cargo.toml`, así que el `.exe` sale sin LTO, con símbolos, con muchas codegen-units y con unwinding — pesa ~25 MB.

**Archivo(s).** `src-tauri/Cargo.toml` (al final del archivo; hoy termina en la línea 88 con la dep de `tauri-plugin-android-fs`).

**Estado actual.** No existe ninguna sección `[profile.release]` en el archivo (confirmado leyendo `Cargo.toml` completo).

**Cambio.** Agregar al final de `Cargo.toml`:
```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
opt-level = "s"
```

**Por qué.** `lto = true` + `codegen-units = 1` permiten inlining/dead-code-elimination cross-crate; `strip = true` saca símbolos de debug; `panic = "abort"` elimina las tablas de unwinding (varios MB) y aborta directo en panic; `opt-level = "s"` optimiza por tamaño (bueno para una app de escritorio que no es hot-loop-bound). Esperado: el `.exe` de release baja de ~25 MB a ~8–12 MB.

**Por qué el panic hook sigue funcionando.** El hook instalado en `lib.rs:650` con `std::panic::set_hook` se ejecuta ANTES de que el runtime desenrolle/aborte — `panic = "abort"` cambia lo que pasa DESPUÉS del hook (aborta en vez de unwind), no impide que el hook corra. Es decir, `crash.log` (lib.rs:682) se sigue escribiendo en cada panic. Sí desaparece el `original_hook(info)` de unwinding por-frame, pero eso solo afecta al backtrace del handler default, no al `Backtrace::force_capture()` explícito del hook (lib.rs:675), que sigue capturando.

**Cuidado con.**
- `panic = "abort"` es incompatible con tests que usen `#[should_panic]` bajo el perfil `test`; pero eso afecta al perfil `test`, no a `release`. Si hay tests con `should_panic`, no se ven afectados porque `cargo test` usa el perfil `dev`/`test`, no `release`.
- `lto = true` + `codegen-units = 1` alargan el tiempo de compilación de release notablemente (esperado, no es un bug).
- **KEEP** `.cargo/config.toml` con `-C target-feature=+crt-static` (necesario para que el `.exe` portable no pida VCRUNTIME140.dll). No lo toques.
- NO agregues `[profile.dev]` cambios — el build de desarrollo debe seguir siendo rápido.

---

### Tarea 0.5 — Autostart: re-registrar al `.exe` actual en el arranque

**Problema.** La app registra autostart con `tauri-plugin-autostart` (call site en `lib.rs:1014-1017`). La entrada `HKCU\...\Run` del usuario apunta a un `.exe` v0.8.1 que ya no existe (path stale de una versión vieja): Windows intenta autostartear un binario inexistente y falla silenciosamente. El plugin, al `enable()`, escribe el path del `.exe` ACTUAL; el problema es que nunca se re-ejecuta ese `enable()` en arranques posteriores, así que la entrada vieja queda.

**Archivo(s).** Call site del plugin: `src-tauri/src/lib.rs:1014-1017`. Comando que hoy hace enable/disable: `src-tauri/src/lib.rs:370-391` (`set_start_with_windows`). Persistencia de la preferencia: `settings.rs:118-125` (`set_start_with_windows`) y el campo `start_with_windows` (`settings.rs:22`). El `settings_store` ya está disponible en el `setup` (`lib.rs:1265`, `settings_for_server = settings_store.clone()`).

**Estado actual (registro del plugin).**
```rust
.plugin(tauri_plugin_autostart::init(
    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
    Some(vec!["--autostart"]),
));
```
**Estado actual (enable/disable, `set_start_with_windows`).**
```rust
#[cfg(desktop)]
{
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if value {
        manager.enable().map_err(|e| format!("autostart enable: {e}"))?;
    } else {
        manager.disable().map_err(|e| format!("autostart disable: {e}"))?;
    }
}
```

**Cambio.** En el `setup` de `lib.rs`, después de que `settings_store` esté cargado y el plugin registrado (ubicalo cerca del bloque `0a.4` de `--autostart`, `lib.rs:1094-1102`, o justo después de `app.manage(AppState{…})` — cualquier punto donde `app.handle()` y `settings_store` existan), re-registrar el autostart si la preferencia está activa. El patrón exacto usa `ManagerExt`:

```rust
// 0a.5 Reparar autostart stale: si el usuario tiene "start with Windows"
//      activo, re-registrar apuntando al .exe ACTUAL. Una entrada de una
//      versión vieja (path que ya no existe) queda saneada porque
//      disable() la borra y enable() la reescribe con el exe actual.
#[cfg(desktop)]
{
    use tauri_plugin_autostart::ManagerExt;
    let want_autostart = settings_store.snapshot().start_with_windows;
    let manager = app.autolaunch();
    if want_autostart {
        // disable() primero limpia la entrada existente (stale o no);
        // enable() la reescribe con std::env::current_exe() actual.
        let _ = manager.disable();
        if let Err(e) = manager.enable() {
            runtime_log::err(format!("[autostart] re-register failed: {e}"));
        } else {
            runtime_log::info("[autostart] re-registered to current exe path");
        }
    } else {
        // El usuario NO quiere autostart: garantizar que no quede una
        // entrada stale de una versión previa.
        if let Ok(true) = manager.is_enabled() {
            let _ = manager.disable();
            runtime_log::info("[autostart] removed stale entry (pref is off)");
        }
    }
}
```

`app.autolaunch()` viene de `tauri_plugin_autostart::ManagerExt` (el mismo trait que ya usa `set_start_with_windows`). Los métodos son `enable() -> Result<()>`, `disable() -> Result<()>`, `is_enabled() -> Result<bool>`. `enable()` internamente resuelve `std::env::current_exe()`, por eso re-ejecutarlo repara el path.

**Por qué.** `tauri-plugin-autostart` escribe la entrada `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` con el path del binario en el momento del `enable()`. Como el usuario habilitó autostart con v0.8.1, la entrada quedó clavada a ese path. Reejecutar `disable()` + `enable()` en cada arranque (cuando la pref está activa) fuerza a reescribir el path al `current_exe()` de la versión corriendo, saneando la entrada stale. El branch `else` limpia entradas fantasma cuando la pref está apagada.

**Cuidado con.**
- Hacé `disable()` best-effort (`let _ =`) porque si no hay entrada previa devuelve error/ok según plataforma; lo que importa es que `enable()` termine dejando la entrada correcta.
- El flag de lanzamiento debe seguir siendo `--autostart` (coincide con `Some(vec!["--autostart"])` del `init` y con la detección `std::env::args().any(|a| a == "--autostart")` de `lib.rs:1096`). NO cambies ese string.
- Esto corre en `#[cfg(desktop)]`. En Android no hay autolaunch — no lo compiles ahí.
- **KEEP** el comando `set_start_with_windows` (`lib.rs:370`) tal cual — sigue siendo el toggle manual del usuario; esta tarea solo agrega la reparación al arranque.
- Si preferís no re-registrar en CADA arranque (I/O de registro), una variante equivalente y más barata: comparar el path guardado en la entrada `Run` contra `std::env::current_exe()` y solo re-registrar si difieren; pero eso requiere leer el registro con `winreg` (ya es dep en Windows, `Cargo.toml:76`). La variante `disable()`+`enable()` incondicional es más simple y suficiente.

## Cómo verificar

1. **Poller (0.1).** Compilar y correr (ver `../00-SHARED-CONTEXT.md` para el comando de build/run). Copiar una imagen grande al portapapeles SIN ningún peer con clipboard-sync habilitado. Abrir el Task Manager: la CPU del proceso `millennium-clipboard.exe` en reposo debe estar ~0% (antes: picos constantes). En el log NO debe aparecer actividad de encode por tick. Luego habilitar clipboard-sync con un peer y copiar texto: debe propagarse. Copiar un texto de >1 MB: en el runtime log debe aparecer `[clipboard] text too large (… bytes) — skipped` y NO debe intentar mandarlo como imagen.
2. **Logs (0.2).** Con el panel de log CERRADO, provocar líneas de backend (p.ej. desconectar un peer para disparar `[poll] probe failed`). Confirmar que NO llega el evento `log-line` al frontend mientras está cerrado (agregá temporalmente un `console.count('log-line')` en el listener de `main.js:1816` — no debe incrementar con el panel cerrado). Abrir el panel: `openLogModal` trae el historial vía `get_runtime_log` y a partir de ahí sí llegan eventos vivos. Confirmar que `[poll] probe failed` repetido no aparece una vez por segundo sino deduplicado. Dejar la app corriendo horas: el uso de RAM del webview no debe crecer sin techo (el ring corta en 2000 líneas).
3. **FX (0.3).** Minimizar / cambiar de pestaña: con DevTools del webview (o Task Manager → GPU) confirmar que el repaint cae a ~0 (la clase `fx-paused` aparece en `<html>` al ocultar). En Settings, activar el toggle de FX-off: el grid, scanline y noise desaparecen y las animaciones se detienen; recargar la app y confirmar que la preferencia persiste (localStorage). Inspeccionar `.card` y confirmar que ya NO tiene `backdrop-filter` (usar el inspector de estilos computados). El typewriter no debe seguir corriendo con la ventana oculta.
4. **Build (0.4).** `cargo build --release` (desde `src-tauri`, o el comando del shared context). Medir el tamaño del `.exe` resultante: debe estar en ~8–12 MB (antes ~25 MB). Forzar un panic de prueba (temporalmente) y confirmar que `%APPDATA%/com.guidocameraeq.millennium/crash.log` se sigue escribiendo — prueba de que el panic hook corre bajo `panic="abort"`.
5. **Autostart (0.5).** Con `start_with_windows` activo, arrancar la app. Abrir `regedit` → `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` y confirmar que la entrada de Millennium apunta al `.exe` ACTUAL (no al path v0.8.1). En el runtime log debe verse `[autostart] re-registered to current exe path`. Alternativamente: `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v millennium-clipboard` (o el nombre real de la entrada) y verificar el path.

**Test unitario sugerido (0.1).** No es trivial testear arboard sin portapapeles real, pero sí se puede testear el gate de tamaño: extraer la lógica "¿este texto es sincronizable?" a una función pura `fn is_syncable_text(t: &str) -> bool { !t.is_empty() && t.len() <= 1_000_000 }` y agregar `#[test] fn rejects_oversize_text() { assert!(!is_syncable_text(&"x".repeat(1_000_001))); assert!(is_syncable_text("hola")); assert!(!is_syncable_text("")); }`.

## Riesgo y rollback

- **0.1 (poller)** es el cambio de mayor riesgo: reestructura el hilo de sincronización. Si algo falla, la sincronización de clipboard deja de andar (no rompe el resto de la app). Rollback: revertir `spawn_clipboard_poller` a la versión con `spawn_blocking` + `interval(500ms)`. Se puede shipear independiente de las demás. Riesgo secundario: la semántica del hash de imagen (RGBA crudo vs PNG) — mitigado verificando el lado receptor antes de cambiar qué hash viaja (ver **Cuidado con** de 0.1).
- **0.2 (logs)** es de bajo riesgo: el peor caso es no ver líneas en vivo si el toggle `set_log_panel_open` se desincroniza — mitigado porque `openLogModal` siempre re-trae el historial completo con `get_runtime_log`. Frontend y backend se pueden shipear por separado (el ring array funciona aunque el backend siga emitiendo siempre).
- **0.3 (FX)** es visual y reversible: si el grid con `translateY` se ve mal, revertir a `grid-flow`/`background-position`. Cada sub-cambio (backdrop-filter, mix-blend-mode, media query, toggle) es independiente y se puede revertir aislado. No afecta funcionalidad.
- **0.4 (build)** solo cambia flags de compilación. Rollback: borrar el `[profile.release]`. Riesgo: si algún crate no soporta `panic="abort"` (raro), el build falla en compile-time — se detecta de inmediato, no en runtime. Seguro de shipear solo.
- **0.5 (autostart)** de bajo riesgo: en el peor caso re-registra una entrada de registro idéntica en cada arranque (I/O trivial). Rollback: quitar el bloque `0a.5`. Independiente de todo lo demás.
- **Orden de shipeo seguro:** 0.4 y 0.5 primero (aislados, bajo riesgo), luego 0.2, luego 0.3, y 0.1 al final con testing manual de sincronización real entre dos máquinas.
