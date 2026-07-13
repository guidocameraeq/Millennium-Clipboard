# Millennium Clipboard — Android · Fase C: Portapapeles, QR y UI móvil

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Android (con toques de Rust compartido y CSS que también afecta Windows) · **Prerrequisitos:** Fase A (lifecycle/aprobación) y Fase B (discovery/storage) aplicadas; la Fase 3 de Windows (CSP estricta) consume la Tarea C.4 · **Esfuerzo:** grande (~1.5–2 días; la Tarea C.3 es la más pesada) · **Riesgo:** med (C.3 reescribe todo el CSS móvil; C.4 toca fuentes que también usa Windows)

## Objetivo
Hacer que en Android el portapapeles funcione dentro de los límites del OS (leer al recuperar foco, escribir al recibir), que el escaneo de QR realmente empareje, y que la UI móvil deje de estar rota (tres sistemas de CSS peleando, selectores muertos, viewport que bloquea el zoom). De paso, self-hostear las fuentes para que una app "NO CLOUD" no pida nada a la red y para desbloquear la CSP estricta de Windows Fase 3.

## Definición de "hecho"
- [ ] En Android, con el toggle **CLIPBOARD SYNC** de un peer en ON, copiar texto en el teléfono y volver a la app (recuperar foco) hace que ese texto se pushee al peer; el log muestra la línea de push.
- [ ] En Android, recibir un push de texto escribe el portapapeles del sistema vía `clipboard_manager.write_text` y se puede pegar en otra app.
- [ ] El toggle per-peer de clipboard sync **es visible** en Android (ya no lo esconde el CSS).
- [ ] La ruta de imagen-a-galería en `handle_clipboard_image` está o bien gateada correctamente (ya no es código muerto) o eliminada; no queda un `#[cfg(target_os = "android")]` que nunca se alcanza.
- [ ] Escanear un QR con la cámara en Android empareja al peer: usa `Format.QRCode`, lee `cur.camera`, muestra un `confirm()` async del plugin-dialog (no `window.confirm`), y loguea errores vía `record_frontend_log`.
- [ ] Todos los `window.confirm`/`alert` de los tres sitios (QR pair, forget-peer, apply-update) usan el `confirm()` async de `@tauri-apps/plugin-dialog` y funcionan en Android.
- [ ] `styles.css` tiene **un** sistema móvil: base = teléfono, **una** media query `min-width` para desktop. Se borró el mirror `html.is-mobile … !important` y el selector universal `html.is-mobile *`.
- [ ] Los selectores móviles muertos (`.compose-area`/`.send-btn`/`.status-msg`) apuntan a los reales (`.textarea-wrap`/`.transmit-btn`/`#text-composer`/`#send-btn`/`#status-msg`).
- [ ] `.modal-actions` tiene padding de safe-area y los modales full-screen usan `100dvh`.
- [ ] El `<meta viewport>` ya no tiene `maximum-scale` ni `user-scalable=no`.
- [ ] Las fuentes (Orbitron, Audiowide, Share Tech Mono, JetBrains Mono) se sirven como `@font-face` local; `index.html` ya no tiene `<link>` a `fonts.googleapis.com` ni `fonts.gstatic.com`. La app arranca sin red y con las tipografías correctas.
- [ ] `npm run tauri android build --apk` compila; `cd src-tauri; cargo check` compila.

---

## Tareas

### Tarea C.1 — Portapapeles Android dentro de los límites del OS

**Problema.** En Android el `spawn_clipboard_poller` es un stub que no hace nada (`lib.rs:705-716`). Desde Android 10 una app **no puede leer el portapapeles en background**, pero **sí puede leerlo mientras está en foreground**. La escritura al recibir ya está implementada (`http_server.rs:797-806` usa `clipboard_manager.write_text`) pero es **inalcanzable**: el handler está gateado por `is_enabled(&sender_fingerprint)` (`http_server.rs:777`) y ese flag nunca puede ponerse en `true` en Android porque el CSS esconde el único toggle que lo prende (`styles.css:2021-2029`). La rama de imagen-a-galería (`http_server.rs:893-924`) es, por la misma razón, código muerto hoy.

Solución: (a) leer el portapapeles **on-focus** desde el frontend (permitido en foreground) y pushear el cambio; (b) surface el toggle per-peer en Android; (c) decidir la rama de imagen (gatear o borrar). La escritura on-receive ya existe y queda intacta.

**Archivo(s).** `src/main.js`, `src-tauri/src/lib.rs:705-716`, `src-tauri/src/http_server.rs:843-949`, `src-tauri/capabilities/mobile.json`, `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MainActivity.kt`, `src/styles.css:2021-2029`.

**Estado actual.**

Stub del poller Android (`lib.rs:705-716`):
```rust
#[cfg(target_os = "android")]
fn spawn_clipboard_poller(
    _peers: discovery::PeerMap,
    _store: Arc<clipboard_sync::ClipboardSyncStore>,
    _my_alias: String,
    _my_fingerprint: String,
) {
    // Android: clipboard polling in background is restricted by the
    // OS since Android 10. We'll wire this up via tauri-plugin-clipboard-manager
    // in a later iteration when the foreground service lands.
    runtime_log::info("[clipboard] poller disabled on Android (handled by foreground service later)");
}
```

Escritura on-receive, ya implementada y correcta (`http_server.rs:797-806`):
```rust
#[cfg(target_os = "android")]
let written: Result<Result<(), String>, tokio::task::JoinError> = {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let result = state
        .app
        .clipboard()
        .write_text(payload.text.clone())
        .map_err(|e| e.to_string());
    Ok(result)
};
```

CSS que esconde el toggle (`styles.css:2021-2029`):
```css
/* Clipboard sync is desktop-only by design ... Hide every clipboard-sync
   surface on Android: the per-peer toggle on cards and the toggle
   row inside the peer-details modal. */
html.is-mobile .clip-toggle,
html.is-mobile #peer-details-clip-row {
  display: none !important;
}
```

Rama de imagen-a-galería inalcanzable (`http_server.rs:893-924`, resumida): decodifica el PNG y llama `crate::android_fs_bridge::save_image_to_gallery(&app_handle, &filename, &bytes_for_save)`. `save_image_to_gallery` es real (`android_fs_bridge.rs:72`).

**Cambio.**

**(a) Push on-focus desde el frontend.** No toques el stub `spawn_clipboard_poller` (dejalo como no-op con el log; el poller de fondo sigue prohibido). En su lugar, la lectura pasa por el frontend cuando la app está en foreground. Necesitás un comando Rust que reciba el texto leído y lo broadcastee a los peers opt-in. Reutilizá la lógica del poller desktop: extraé la parte que "toma un `String`, deduplica vía el `ClipboardSyncStore`, y hace el POST a cada peer con clipboard sync ON" a una función `async fn broadcast_clipboard_text(...)` y exponé un comando:

```rust
// lib.rs — nuevo comando Tauri, registrarlo en el invoke_handler![...]
#[tauri::command]
async fn push_local_clipboard(
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    if text.is_empty() || text.len() > 1_000_000 {
        return Ok(()); // fuera de rango: ignorar en silencio
    }
    let hash = clipboard_sync::hash_text(&text);
    // Supresión de eco: si lo acabamos de recibir/enviar, no re-broadcast.
    if state.clipboard.is_recent(&hash) {
        return Ok(());
    }
    state.clipboard.note_synced(hash);
    // Reutilizá el mismo camino que usa el poller desktop para postear a
    // cada peer con clipboard_sync habilitado. Cloná lo que necesites del
    // Mutex ANTES del await (convención §4 del contexto compartido).
    broadcast_clipboard_text(&state, &text).await;
    Ok(())
}
```

> Antes de escribir esto, **leé el cuerpo del poller desktop** (`lib.rs:718-760+`, la parte después de obtener el `ClipSnapshot::Text`) para ver exactamente cómo arma el POST (endpoint `/clipboard`, payload `ClipboardPayload { text, sender_alias, sender_fingerprint }`, sobre qué peers itera y cómo filtra por `clipboard_sync`). `broadcast_clipboard_text` debe hacer lo mismo. Confirmá los métodos reales de `ClipboardSyncStore` (`is_recent`, `note_synced`, `hash_text`) en `clipboard_sync.rs` — usá los que existan; no inventes nombres.

En el frontend, leé el portapapeles al recuperar foco usando el plugin-clipboard-manager (el global ya está expuesto por `withGlobalTauri`). Agregá cerca del bloque de listeners de clipboard (`main.js:1939-1959`):

```js
// Android: no podemos sondear el portapapeles en background (OS lo
// prohíbe desde Android 10), pero SÍ podemos leerlo cuando la app
// vuelve a foreground. Al recuperar foco/visibilidad leemos el texto
// y lo empujamos a los peers con clipboard sync ON (el backend
// deduplica y filtra por consentimiento).
if (/android/i.test(navigator.userAgent)) {
  const clipboard = window.__TAURI__ && window.__TAURI__.clipboardManager;
  let lastPushed = '';
  async function pushClipboardOnFocus() {
    if (!clipboard || !clipboard.readText) return;
    try {
      const text = await clipboard.readText();
      if (!text || text === lastPushed) return;
      lastPushed = text;
      await invoke('push_local_clipboard', { text });
    } catch (err) {
      reportToBackend('WARN', `clipboard read on focus failed: ${err}`);
    }
  }
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') pushClipboardOnFocus();
  });
  window.addEventListener('focus', pushClipboardOnFocus);
}
```

Y quitá el early-return que hoy silencia los eventos de clipboard entrante en Android (`main.js:1946` y `1954`, las líneas `if (_clipboardIsAndroid) return;`): en Android **sí** queremos el status/notify cuando llega texto, porque ahora la escritura on-receive es alcanzable. Dejá el `_clipboardIsAndroid` solo si querés seguir suprimiendo la notificación de **imagen** (ver punto (c)); para texto, eliminá el guard.

**(b) Hook Kotlin de foco (refuerzo).** El `visibilitychange`/`focus` de la WebView cubre el caso normal, pero para robustez agregá el override en `MainActivity.kt` (editable; NO toques `WryActivity.kt` que es auto-generado). `WryActivity` ya define `onWindowFocusChanged` (línea 121) y llama `Rust.onWindowFocusChanged`; vos lo extendés:

```kotlin
// MainActivity.kt — agregar dentro de class MainActivity : TauriActivity()
override fun onWindowFocusChanged(hasFocus: Boolean) {
  super.onWindowFocusChanged(hasFocus)
  // Al recuperar foco, empujamos un evento JS 'window focus' redundante
  // para disparar el read-on-focus aunque la WebView no haya emitido su
  // propio evento 'focus' (algunos builds de WebView lo tragan tras un
  // Intent de cámara / cambio de app).
  if (hasFocus) {
    runOnUiThread {
      window.decorView.post {
        // dispatch a synthetic focus so the JS listener fires
        // (evaluamos JS mínimo; el read real vive en el frontend)
      }
    }
  }
}
```

> **Nota de implementación:** `mWebView` es `private` en `WryActivity`, así que `MainActivity` no puede llamar `mWebView.evaluateJavascript(...)` directo. Dos opciones válidas: (i) confiar solo en el `window 'focus'`/`visibilitychange` del frontend (punto (a)) y dejar este override como no-op documentado — es lo más simple y ya cubre el 95% de los casos; (ii) si querés el disparo Kotlin real, obtené la WebView vía `findViewById`/recorriendo el árbol de vistas y llamá `evaluateJavascript("window.dispatchEvent(new Event('focus'))", null)`. **Preferí (i)**: menos superficie, cero riesgo de tocar internals de Tauri. El bloque de arriba puede quedar simplemente como `if (hasFocus) { /* frontend focus listener handles clipboard read */ }` o directamente no agregar el override. Documentá la decisión en el commit.

**(c) Toggle visible + rama de imagen.** Borrá el bloque CSS `styles.css:2021-2029` que esconde `#peer-details-clip-row` (y el `.clip-toggle`) en móvil. Como la Tarea C.3 reescribe todo el CSS móvil, en la práctica **no vuelvas a portar esta regla** al stylesheet nuevo. El toggle per-peer (`main.js:1504-1515`, comando `set_clipboard_sync`) ya funciona; solo estaba oculto.

Para la rama de imagen (`http_server.rs:893-924`): **gateala correctamente**, no la borres — el trabajo (`save_image_to_gallery`) es real y útil. Ya está gateada por el mismo `is_enabled(&sender_fingerprint)` (`http_server.rs:850`) que el texto, así que en cuanto el toggle sea prendible en Android, deja de ser código muerto. **Lo único a arreglar:** el `eprintln!` que usa (`914`, `917`, `929`, `933`) viola la convención §4 (usar `runtime_log`). Reemplazá cada `eprintln!("[clipboard] ...")` de este handler por `runtime_log::info(...)`/`runtime_log::err(...)`. Si preferís no mantener imagen en Android por ahora, la alternativa mínima es devolver `StatusCode::NOT_IMPLEMENTED` en Android antes de decodificar y suprimir la notificación de imagen en el frontend — pero dado que la ruta ya existe y compila, **preferí gatear + arreglar el logging**.

**(d) Capabilities.** Agregá a `src-tauri/capabilities/mobile.json` los permisos de clipboard-manager para que la lectura/escritura estén habilitadas explícitamente en Android:

```json
"permissions": [
  "barcode-scanner:default",
  "barcode-scanner:allow-scan",
  "barcode-scanner:allow-cancel",
  "barcode-scanner:allow-check-permissions",
  "barcode-scanner:allow-request-permissions",
  "clipboard-manager:allow-read-text",
  "clipboard-manager:allow-write-text",
  "android-fs:default"
]
```

> `default.json` ya concede `clipboard-manager:allow-read-text/write-text` y no tiene clave `platforms`, así que técnicamente aplica también en Android; agregarlos a `mobile.json` es explícito y a prueba de futuras restricciones de plataforma. `dialog:default` (necesario para la Tarea C.2) ya está en `default.json` sin restricción de plataforma — **no** necesitás agregar dialog a `mobile.json`.

**Por qué.** El portapapeles es la feature estrella y en Android hoy no existe: no se lee (stub) y aunque se escribe, la escritura es inalcanzable por el toggle escondido. Leer on-focus es el patrón correcto dentro de las reglas de Android 10+. Reutilizar el camino de broadcast del poller desktop evita duplicar lógica de red y respeta el store de supresión de eco.

**Cuidado con.**
- **No** sostener el `Mutex` del `PeerMap`/store a través del `await` del POST (convención §4). Cloná la lista de peers destino antes de awaitar.
- **Supresión de eco:** sin `is_recent`, recibir texto → escribir OS clipboard → el `visibilitychange` lo relee → lo re-broadcast → loop. El `note_synced(hash)` que ya hace `handle_clipboard` (`http_server.rs:782`) combinado con el chequeo `is_recent` en `push_local_clipboard` corta el loop. Verificá que ambos usan el **mismo** hashing (`hash_text`).
- `clipboard.readText()` puede rechazar si la app perdió foco justo en el read; el `try/catch` lo maneja, no propagues.
- No rompas el path desktop del poller al extraer `broadcast_clipboard_text` — el poller desktop debe seguir llamándolo igual.
- Registrá `push_local_clipboard` en el `invoke_handler![...]` de `lib.rs` (junto a `record_frontend_log`, `~lib.rs:1431`), o el `invoke` del frontend tira "command not found".

---

### Tarea C.2 — Escaneo de QR: los tres defectos + confirmaciones que funcionen en Android

**Problema.** El flujo de escaneo (`main.js:1285-1354`) tiene tres bugs que lo hacen no emparejar nunca, más el problema transversal de `window.confirm`:
1. Pasa `formats: ['QrCode']` (string). El plugin espera valores del enum `Format`; el string mal escrito hace que no matchee el formato QR.
2. Compara `if (cur !== 'granted')` sobre el **objeto** que devuelve `checkPermissions()` (que es `{ camera: 'granted' | 'denied' | ... }`), así que la condición es siempre `true` — o peor, entra a `requestPermissions()` cada vez.
3. Usa `confirm(...)` nativo (`main.js:1330`), que en el WebView de Android **no muestra diálogo y devuelve `undefined`/`false`** → el pairing se cancela solo.

Además hay dos `window.confirm` más: forget-peer (`main.js:1521`) y apply-update (`main.js:1625`), con el mismo problema en Android. Y los errores de scan se pintan en el DOM pero no se loguean al backend.

**Archivo(s).** `src/main.js:1285-1354` (scan), `src/main.js:1521` (forget), `src/main.js:1625` (update).

**Estado actual (scan, `main.js:1291-1330`).**
```js
        // Make sure the user has granted CAMERA permission first.
        if (barcodeScanner.checkPermissions && barcodeScanner.requestPermissions) {
          const cur = await barcodeScanner.checkPermissions();
          if (cur !== 'granted') {
            const req = await barcodeScanner.requestPermissions();
            if (req !== 'granted') {
              qrAddError.textContent = 'Camera permission denied.';
              qrAddError.hidden = false;
              return;
            }
          }
        }
        qrScanBtn.disabled = true;
        qrScanBtn.textContent = '▸ POINT AT QR…';
        const res = await barcodeScanner.scan({ formats: ['QrCode'] });
        ...
        closeQrModal();
        const ok = confirm(`Pair with ${confirmLabel}?\n\nIt will be added as a favourite peer.`);
        if (!ok) {
```

**Estado actual (forget, `main.js:1521`).**
```js
    const ok = confirm(
      `Forget peer "${peer.name}"?\n\n` +
      ...
    );
    if (!ok) return;
```

**Estado actual (update, `main.js:1625`).**
```js
    const ok = confirm(promptMsg);
    if (!ok) return;
```

**Cambio.**

Como el proyecto usa `withGlobalTauri`, el plugin-dialog está en `window.__TAURI__.dialog` (ya capturado en `const dialog = window.__TAURI__.dialog;`, `main.js:34`) y expone `confirm(message, options)` async. El barcode-scanner está en `window.__TAURI__.barcodeScanner` y su enum `Format` está en el mismo namespace del plugin. Definí un helper de confirm arriba de todo (cerca de `notify`, `main.js:41`):

```js
// Confirmación cross-platform. El confirm() nativo del WebView de Android
// no muestra diálogo (devuelve false), así que usamos el del plugin-dialog,
// que renderiza un AlertDialog nativo en Android y una ventana en Windows.
async function confirmDialog(message, title = 'Confirm') {
  if (dialog && dialog.confirm) {
    try { return await dialog.confirm(message, { title, kind: 'warning' }); }
    catch (_) { /* cae al confirm nativo abajo */ }
  }
  return window.confirm(message);
}
```

**Scan** — reescribí el bloque `main.js:1291-1330` así:

```js
        // Chequear permiso de CÁMARA. checkPermissions() devuelve un
        // objeto { camera: 'granted' | 'denied' | 'prompt' ... }.
        if (barcodeScanner.checkPermissions && barcodeScanner.requestPermissions) {
          const cur = await barcodeScanner.checkPermissions();
          if (cur.camera !== 'granted') {
            const req = await barcodeScanner.requestPermissions();
            if (req.camera !== 'granted') {
              qrAddError.textContent = 'Camera permission denied.';
              qrAddError.hidden = false;
              reportToBackend('WARN', 'QR scan: camera permission denied');
              return;
            }
          }
        }
        qrScanBtn.disabled = true;
        qrScanBtn.textContent = '▸ POINT AT QR…';
        const { Format } = window.__TAURI__.barcodeScanner;
        const res = await barcodeScanner.scan({ formats: [Format.QRCode] });
        ...
        closeQrModal();
        const ok = await confirmDialog(
          `Pair with ${confirmLabel}?\n\nIt will be added as a favourite peer.`,
          'Pair peer'
        );
        if (!ok) {
```

Y en el `catch (err)` del scan (`main.js:1347-1349`), agregá el log al backend:
```js
      } catch (err) {
        qrAddError.textContent = `Scan failed: ${err}`;
        qrAddError.hidden = false;
        reportToBackend('ERR', `QR scan failed: ${err}`);
      } finally {
```

> `reportToBackend(level, msg)` ya existe (`main.js:1386-1390`) y hace `invoke('record_frontend_log', { level, msg })`. Usalo; no reimplementes.

**Forget** (`main.js:1521`) — cambiá `const ok = confirm(...)` por `const ok = await confirmDialog(..., 'Forget peer');` (el callback del listener ya es `async`, `main.js:1517`).

**Update** (`main.js:1625`) — cambiá `const ok = confirm(promptMsg);` por `const ok = await confirmDialog(promptMsg, 'Apply update');` (el callback ya es `async`, `main.js:1619`).

**Por qué.** Con los tres fixes, escanear un QR: (1) matchea el formato QR real, (2) evalúa bien el permiso de cámara, (3) muestra un diálogo de confirmación que en Android sí aparece y devuelve un booleano real. Los otros dos confirms comparten el bug de Android; unificarlos en `confirmDialog` los arregla de una. Loguear el error de scan deja rastro en el mismo buffer que el usuario pega para reportar.

**Cuidado con.**
- Confirmá el nombre exacto del miembro del enum. En `@tauri-apps/plugin-barcode-scanner` v2 el enum `Format` tiene el miembro **`QRCode`** (no `QrCode`, no `QRcode`). Si al compilar/ejecutar `Format.QRCode` es `undefined`, inspeccioná `window.__TAURI__.barcodeScanner.Format` en la consola del WebView y usá el nombre real; anotá la corrección.
- `dialog.confirm` en plugin-dialog v2 devuelve `Promise<boolean>` y acepta `{ title, kind }`. `kind` válido: `'info' | 'warning' | 'error'`. No pases `type` (API vieja).
- No borres el `closeQrModal()` antes del confirm: el comentario del código explica que el confirm debe quedar sobre la pantalla normal, no detrás del overlay de cámara. Mantené ese orden.
- El fallback a `window.confirm` en `confirmDialog` es inofensivo en desktop; no lo quites (cubre el caso de que el plugin no esté cargado).

---

### Tarea C.3 — Reconstruir el CSS móvil: un solo sistema mobile-first

**Problema.** Hay **tres sistemas de CSS móvil peleando** (`styles.css`):
1. La media query `@media (max-width: 768px)` (`1585-1782`).
2. Un mirror `html.is-mobile … !important` (`1802-2162`) que reimplementa lo estructural con `!important` "por si el parser del WebView descarta la media query".
3. Un selector universal `html.is-mobile * { min-width: 0 !important; max-width: 100% !important; }` (`1875-1878`) que nuclea el ancho de **todo** elemento.

Esto es imposible de razonar: cada regla móvil existe 2–3 veces, algunas con `!important` y otras no, y el `*` universal rompe layouts legítimos (iconos, thumbnails). Encima, la media query apunta a **selectores muertos**: `.compose-area textarea` (`1699`), `.send-btn` (`1705`, `1852`), `.status-msg` (`1714`) — el markup real usa `.textarea-wrap` + `#text-composer`, `.transmit-btn`/`#send-btn`, y `#status-msg` dentro de `.status-strip` (verificado en `index.html:138-188`). Así que las reglas móviles del composer/status **no aplican a nada**.

**Archivo(s).** `src/styles.css` (base desktop `~155-1560`, media queries `1585-1782`, mirror `1790-2162`), `src/index.html` (markup de referencia).

**Estado actual (selectores muertos, `styles.css:1699-1714`).**
```css
  .compose-area textarea {
    min-height: 120px;
    font-size: 14px;
    -webkit-text-size-adjust: 100%;
  }
  ...
  .send-btn {
    min-height: 48px;
    font-size: 12px;
    flex: 1 1 100%;
    order: 99;
  }
  ...
  .status-msg { font-size: 10px; }
```

**Estado actual (el `*` universal, `styles.css:1875-1878`).**
```css
html.is-mobile * {
  min-width: 0 !important;
  max-width: 100% !important;
}
```

**Markup real (index.html):** `.textarea-wrap` > `textarea#text-composer` (`138-146`); `button.transmit-btn#send-btn` (`175`); `.status-strip` > `span.status-text#status-msg` (`188`); toggle row `#peer-details-clip-row` (`301`).

**Cambio.** Reescritura **mobile-first**. La estrategia:

1. **La base (sin media query) = teléfono.** Mové los defaults de layout que hoy están pensados para desktop hacia una **única** `@media (min-width: NNNpx)` (desktop). Concretamente:
   - El `.app` desktop es `display:grid; grid-template-rows:auto 1fr auto; height:100vh; padding:18px 22px` (`155-163`). En la base mobile-first, `.app` debe ser `display:flex; flex-direction:column; min-height:100dvh; padding:0` (lo que hoy fuerza el mirror `html.is-mobile .app`). El `grid + height:100vh` desktop pasa a vivir dentro de `@media (min-width: 900px)`.
   - Idem `.hud` (base = columna sticky; desktop = `grid-template-columns:1fr auto 1fr`), `.grid` (base = flex column; desktop = lo que sea el layout ancho), `.hud-right`, `.peers`, `.composer`, `.modal`, `.modal-actions`, `.settings-row`, `.status-strip`.

2. **Borrá enteros** los bloques `html.is-mobile …` (`1790-2162`) y el `@media (max-width: 768px)`/`(max-width: 380px)` viejos (`1585-1782`), incluido el selector universal `html.is-mobile *`. También el bloque que esconde el clipboard toggle (`2021-2029`, ya cubierto por C.1) y el bloque `.clip-toggle`/`#peer-details-clip-row`.

3. **Un solo breakpoint desktop.** Todo lo "desktop-specific" (la grilla, paddings amplios, hover transforms, columnas del HUD) va en **una** `@media (min-width: 900px) { … }`. El `900px` es el mismo umbral que usa el script inline de `is-mobile` (`index.html:18`, `innerWidth <= 900`) — mantené coherencia. En mobile-first, la base no lleva `!important`; el desktop query gana por orden de cascada al venir después, sin `!important`.

4. **Arreglá los selectores muertos.** Donde el CSS viejo decía `.compose-area textarea` / `.send-btn` / `.status-msg`, usá los reales:
   - `.compose-area textarea` → `#text-composer` (o `.textarea-wrap textarea`).
   - `.send-btn` → `.transmit-btn` (o `#send-btn`).
   - `.status-msg` → `#status-msg` (dentro de `.status-strip`).

5. **safe-area en `.modal-actions` + `100dvh`.** El `.modal-actions` base (`1409-1415`) no tiene safe-area; en móvil el botón inferior queda bajo la gesture bar. Y los modales full-screen usan `100vh` (`1721-1724`), que en Android incluye la barra de URL colapsable y "salta". Cambiá a `100dvh`:

```css
/* Base (mobile-first): modales ocupan la pantalla real (dvh, no vh). */
.modal,
.settings-modal-wide,
.log-modal {
  width: 100vw;
  height: 100dvh;
  max-width: 100vw;
  max-height: 100dvh;
  border-radius: 0;
}

.modal-actions {
  display: flex;
  flex-direction: column;   /* botones apilados en teléfono */
  gap: 8px;
  padding: 12px 18px;
  /* respetá la gesture bar / notch inferior */
  padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px));
  border-top: 1px solid var(--neon-cyan-soft);
  background: rgba(0, 8, 20, 0.5);
}

/* Desktop: modal centrado, acciones en fila. */
@media (min-width: 900px) {
  .modal { width: min(560px, 92vw); height: auto; max-height: 80vh; border-radius: 0; }
  .modal-actions { flex-direction: row; padding-bottom: 12px; }
}
```

> El HUD sticky ya usa `env(safe-area-inset-top/left/right)` en el mirror (`1946-1955`); portá eso a la base (sin `!important`). El `.status-strip` inferior debe llevar `padding-bottom: calc(8px + env(safe-area-inset-bottom, 0px))`.

**Estructura destino del archivo (esqueleto):**
```css
/* ---- BASE = teléfono (sin media query) ---- */
.app { display:flex; flex-direction:column; min-height:100dvh; padding:0; }
.hud { display:flex; flex-direction:column; position:sticky; top:0; z-index:200;
       padding: calc(10px + env(safe-area-inset-top,0px)) 12px 10px; }
.grid { display:flex; flex-direction:column; gap:10px; padding:10px; }
#text-composer { min-height:120px; font-size:14px; -webkit-text-size-adjust:100%; }
.transmit-btn { min-height:48px; }
.status-strip { position:sticky; bottom:0;
                padding-bottom: calc(8px + env(safe-area-inset-bottom,0px)); }
/* … resto de la base mobile … */

/* ---- DESKTOP override, UN solo breakpoint ---- */
@media (min-width: 900px) {
  .app { display:grid; grid-template-rows:auto 1fr auto; height:100vh; padding:18px 22px; gap:14px; }
  .hud { display:grid; grid-template-columns:1fr auto 1fr; align-items:center; gap:24px; padding:10px 18px; }
  .grid { /* layout ancho de dos columnas */ }
  .peer-item:hover { transform: /* el hover desktop */; }
  /* … */
}
```

**Por qué.** Un solo sistema mobile-first elimina la triplicación, borra el `!important` que hace el CSS inmantenible, y mata el `*` universal que rompía elementos legítimos. Arreglar los selectores muertos hace que las reglas del composer/status realmente apliquen (hoy son letra muerta). `100dvh` + safe-area arreglan el "salto" y el botón tapado por la gesture bar en Android. Al no depender de `html.is-mobile`, el layout ya no necesita el script inline de `index.html` (podés dejarlo o quitarlo; ver "Cuidado con").

**Cuidado con.**
- **Windows también usa este CSS.** El breakpoint desktop debe reproducir **exactamente** el layout desktop actual (grid, paddings, hover). Verificá en Windows que nada se movió: mismo `padding:18px 22px`, mismo `grid-template-columns` del HUD, mismos hover transforms. Compará contra `git diff` visual.
- El script inline `is-mobile` en `index.html:15-24` queda **inofensivo** pero inútil tras esta tarea (ya nada lo estiliza). Podés borrar el `<script>` y la clase, PERO si lo dejás no rompe nada. Si lo borrás, verificá que ningún JS lee `classList.contains('is-mobile')` de forma load-bearing — hay un uso en `main.js:26` (solo diagnóstico de log, seguro de dejar/quitar).
- No pierdas reglas que hoy solo viven en el mirror `!important` y **no** en la media query (el comentario en `1797-1800` dice que el mirror solo repite lo "estructural", pero revisá caso por caso: por ej. `html.is-mobile body { height:auto; overflow-y:auto }` (`1927-1933`) es crítico para el scroll vertical y **no** está en la media query). Portá esas a la base.
- `100dvh` es soportado en Android WebView moderno (Chrome 108+) y en el WebView2 de Windows; el `minSdk=24` puede traer WebViews viejas — dejá `100vh` como fallback declarado **antes** de `100dvh` en la misma regla (la última gana donde `dvh` se entiende).
- No toques las capas de fondo animadas ni la estética neón; esta tarea es solo layout/responsive.

---

### Tarea C.4 — Self-host de las Google Fonts como `@font-face` local

**Problema.** `index.html:28-33` trae 4 familias desde `fonts.googleapis.com`/`fonts.gstatic.com`. Una app "NO CLOUD", solo-LAN, **no debe pedir nada a Internet** (rompe la promesa de privacidad y falla si no hay salida a WAN). Además, la CSP estricta que introduce **Windows Fase 3** va a bloquear esos hosts, dejando la app con las fuentes fallback. Hay que servir las fuentes localmente.

**Archivo(s).** `src/index.html:28-33` (los `<link>`), `src/styles.css:33-36` (las `--font-*` vars), nuevo directorio `src/fonts/`.

**Estado actual (`index.html:28-33`).**
```html
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link
    href="https://fonts.googleapis.com/css2?family=Orbitron:wght@400;500;700;900&family=Audiowide&family=Share+Tech+Mono&family=JetBrains+Mono:wght@400;500;700&display=swap"
    rel="stylesheet"
  />
```

**Vars que consumen las familias (`styles.css:33-36`).**
```css
  --font-display: 'Orbitron', 'Audiowide', sans-serif;
  --font-logo: 'Audiowide', 'Orbitron', sans-serif;
  --font-mono: 'JetBrains Mono', 'Share Tech Mono', 'Consolas', monospace;
  --font-tech: 'Share Tech Mono', 'JetBrains Mono', monospace;
```

**Cambio.**

1. **Descargá los `.woff2`** de las 4 familias con los pesos que usa el CSS. Pesos realmente referenciados en el `<link>` actual: Orbitron 400/500/700/900, Audiowide 400 (única), Share Tech Mono 400 (única), JetBrains Mono 400/500/700. Fuentes (todas SIL OFL, redistribuibles): descargalas de sus repos oficiales de Google Fonts (`github.com/google/fonts`) o del `fonts.gstatic.com` una sola vez, y guardá los `.woff2` en `src/fonts/`. Nombralos consistente, p.ej. `orbitron-400.woff2`, `orbitron-700.woff2`, `audiowide-400.woff2`, `sharetechmono-400.woff2`, `jetbrainsmono-400.woff2`, etc.

   > Como agente ejecutor: para conseguir los `.woff2` reales, la vía más simple es abrir `https://fonts.googleapis.com/css2?family=...` (la misma URL del `<link>`) **una vez desde una máquina con red**, leer las URLs `.woff2` de `fonts.gstatic.com` que devuelve, y bajarlas. Si no tenés red en el entorno de ejecución, dejá los `.woff2` como TODO explícito y crea los `@font-face` apuntando a `fonts/…woff2`; el humano deposita los archivos. **No** embebas base64 gigante en el CSS (infla el archivo y rompe el diff).

2. **Agregá los `@font-face`** al inicio de `styles.css` (antes del `:root`), uno por peso:

```css
/* Fuentes self-hosted — la app es NO CLOUD, no pedimos nada a la red.
   Todas SIL OFL, redistribuibles. Ver src/fonts/. */
@font-face {
  font-family: 'Orbitron';
  font-style: normal;
  font-weight: 400;
  font-display: swap;
  src: url('fonts/orbitron-400.woff2') format('woff2');
}
@font-face {
  font-family: 'Orbitron';
  font-style: normal;
  font-weight: 700;
  font-display: swap;
  src: url('fonts/orbitron-700.woff2') format('woff2');
}
/* … 500 y 900 de Orbitron … */
@font-face {
  font-family: 'Audiowide';
  font-style: normal; font-weight: 400; font-display: swap;
  src: url('fonts/audiowide-400.woff2') format('woff2');
}
@font-face {
  font-family: 'Share Tech Mono';
  font-style: normal; font-weight: 400; font-display: swap;
  src: url('fonts/sharetechmono-400.woff2') format('woff2');
}
@font-face {
  font-family: 'JetBrains Mono';
  font-style: normal; font-weight: 400; font-display: swap;
  src: url('fonts/jetbrainsmono-400.woff2') format('woff2');
}
/* … 500 y 700 de JetBrains Mono … */
```

3. **Borrá** los tres `<link>` de `index.html:28-33` (los dos `preconnect` + el `stylesheet`). Las `--font-*` vars (`styles.css:33-36`) **no cambian**: los nombres de familia (`'Orbitron'`, etc.) siguen resolviendo, ahora contra los `@font-face` locales.

**Por qué.** Cumple la promesa "NO CLOUD" (cero requests a WAN al arrancar), hace la app funcional sin salida a Internet, y **desbloquea la CSP estricta de Windows Fase 3** (que necesita `style-src`/`font-src` sin hosts remotos; con las fuentes locales, `font-src 'self'` alcanza). Los pesos exactos evitan la síntesis de negrita/faux-bold del navegador.

**Cuidado con.**
- Serví solo los pesos que el CSS usa realmente (los del `<link>` actual). Bajar 9 pesos de cada familia infla el APK/exe.
- `woff2` está soportado por WebView2 (Windows) y el Android WebView (Chrome ≥ 36); no necesitás `.woff`/`.ttf` de fallback.
- Los `.woff2` son binarios: agregalos al repo (no están gitignored) y confirmá que el bundler de Tauri los incluye. Tauri sirve `src/` tal cual (no hay bundler; `frontendDist`/`devUrl` sirven la carpeta), así que `fonts/*.woff2` referenciados con path relativo desde `styles.css` (que está en `src/`) resuelven a `src/fonts/*.woff2`. Verificá el path relativo: `styles.css` está en `src/`, así que `url('fonts/x.woff2')` apunta a `src/fonts/x.woff2`. Correcto.
- Respetá la licencia OFL: no hace falta incluir el OFL.txt para que funcione, pero es buena práctica dejar un `src/fonts/LICENSE-OFL.txt`.
- **No** conviertas esto en base64 inline: rompería el diff y engordaría `styles.css` a cientos de KB.

---

## Cómo verificar

**Build (siempre):**
```powershell
cd src-tauri; cargo check
# Para el código Android-gated (broadcast_clipboard_text, push_local_clipboard,
# handle_clipboard_image con runtime_log) compilá de verdad para Android:
cd ..; npm run tauri android build --apk
```

**C.1 — Clipboard Android (en dispositivo/emulador, `npm run tauri android dev`):**
1. En el teléfono, en la vista de un peer, abrí peer-details: el toggle **CLIPBOARD SYNC** ahora es **visible** (antes oculto). Prendelo.
2. Copiá un texto en otra app del teléfono, volvé a Millennium (recuperás foco). En el panel de LOG debe aparecer una línea de push/`[clipboard]` y el peer debe recibir el texto (verificalo en el otro dispositivo).
3. Desde el otro dispositivo, pusheá texto a este teléfono: el status/notify aparece (ya no lo silencia el guard) y podés **pegar** ese texto en otra app (confirma `write_text`).
4. Loop check: repetí varias veces; el texto **no** debe rebotar infinitamente (supresión de eco por `is_recent`/`note_synced`). Observá que no hay un chorro de líneas `[clipboard]` en el LOG.
5. `grep` de sanidad: en `http_server.rs`, el handler de imagen ya no usa `eprintln!` (usa `runtime_log`).

**C.2 — QR (en Android):**
1. QR modal → SCAN WITH CAMERA. La cámara abre; apuntá a un QR de otro peer. Debe **matchear** (antes no, por `'QrCode'`).
2. Aparece un **diálogo nativo** de confirmación (AlertDialog), no un confirm fantasma. Aceptá → el peer aparece como favorito. Rechazá → status "cancelled".
3. Forzá un error de scan (cancelá la cámara): en el panel de LOG debe aparecer `[ui] QR scan failed: …` (vía `record_frontend_log`).
4. Forget-peer y Apply-update: ambos muestran diálogo nativo en Android y responden al botón (antes se cancelaban solos).

**C.3 — CSS móvil:**
1. En Windows (`npm run tauri dev`): la UI desktop se ve **idéntica** a antes (grid, HUD de 3 columnas, hover). Redimensioná la ventana angosta (< 900px): cae al layout de teléfono limpio, sin overflow horizontal.
2. En Android: el composer, el botón TRANSMIT y el status-strip ahora **sí** reciben estilos móviles (antes selectores muertos). Los modales llenan la pantalla con `100dvh` sin "saltar" al aparecer/desaparecer la barra de URL; el botón inferior del modal no queda tapado por la gesture bar.
3. `grep -n "html.is-mobile" src/styles.css` → **0 resultados**. `grep -n "!important" src/styles.css` → drásticamente menos (idealmente solo `[hidden]`). `grep -n "\.compose-area\|\.send-btn\b\|\.status-msg\b" src/styles.css` → 0 resultados.

**C.4 — Fuentes:**
1. `grep -n "fonts.googleapis\|fonts.gstatic" src/index.html` → **0 resultados**.
2. Arrancá la app **con la red del dispositivo apagada** (modo avión + sin Wi-Fi, o un entorno sin salida WAN): las tipografías Orbitron/Audiowide/etc. se renderizan igual (no caen a Arial/monospace del sistema). Comparación visual: el logo (Audiowide) y los títulos (Orbitron) deben verse como siempre.
3. En DevTools/WebView, Network: **ningún** request a `gstatic`/`googleapis`. `ls src/fonts/*.woff2` lista los pesos usados.

**Viewport (C.3):** `grep -n "maximum-scale\|user-scalable" src/index.html` → 0 resultados; en Android, pinch-to-zoom sobre un modal ahora **funciona** (accesibilidad).

## Riesgo y rollback

- **C.4 es la más segura e independiente:** cero lógica, solo assets + `@font-face`. Se puede shippear sola. Rollback: restaurar los 3 `<link>` y borrar los `@font-face`. Es prerrequisito de la CSP de Windows Fase 3, así que conviene mergearla antes o junto con esa.
- **C.2 es de bajo riesgo y aislada:** toca solo `main.js`. Si `Format.QRCode` resultara ser otro identificador, el fix es un renombre; el resto (permiso, confirm) es correcto igual. Rollback: revert del bloque de scan y de los tres confirms.
- **C.1 tiene riesgo medio en el backend:** extraer `broadcast_clipboard_text` del poller desktop puede romper el path desktop si te equivocás en el refactor. Mitigá dejando el poller desktop llamando a la nueva función sin cambiar su comportamiento y compilando `cargo check` antes de tocar Android. El nuevo comando y el listener de focus son aditivos (no rompen nada si fallan). El loop de eco es el riesgo real: probalo explícitamente (paso 4 de verificación).
- **C.3 es la de mayor riesgo visual y NO es independiente de Windows:** reescribe CSS que la app de escritorio usa a diario. Hacela en su propio commit, con verificación visual en Windows **antes** de tocar Android, y guardá el `styles.css` viejo (git) para comparar. Si algo desktop se rompe, el rollback es un solo `git checkout` del archivo. Recomendación: shippear C.1/C.2/C.4 primero (cada una aislada), y C.3 al final con revisión visual cuidadosa en ambas plataformas.
- **Todas** dependen de que Fase A/B ya estén aplicadas (lifecycle y storage), porque sin la app viva en foreground el read-on-focus no dispara y sin discovery estable no hay peer a quien pushear.
