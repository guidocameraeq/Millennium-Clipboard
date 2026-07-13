# Millennium Clipboard — Rust compartido + frontend · Fase 3: Seguridad

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Rust compartido + frontend (afecta Windows y Android) · **Prerrequisitos:** ninguno (Tarea 3.1 es prerrequisito para confiar en transferencias Android) · **Esfuerzo:** ~1.5–2 días · **Riesgo:** high (3.1 toca el transporte TLS de TODO envío; el resto es med/low)

## Objetivo

Cerrar los agujeros de seguridad del canal LAN: hoy el cliente HTTPS acepta *cualquier* certificado (`danger_accept_invalid_certs(true)`) y "verifica" al peer con un probe `/info` previo que es trivialmente spoofeable (MITM responde el probe con la fingerprint correcta y luego sirve el payload real desde otro cert). Esta fase implementa **cert pinning real** contra la fingerprint esperada, agrega **CSP** al frontend, elimina inyección de **HTML sin escapar**, cierra el endpoint `/text` **sin consentimiento**, pone **límite de body** a las rutas que hoy pueden agotar RAM, verifica la **integridad del updater**, y endurece `safe_join` contra nombres de dispositivo reservados de Windows y ADS.

## Definición de "hecho"

- [ ] `http_client.rs` ya NO contiene `danger_accept_invalid_certs(true)`. Un envío a un peer cuyo cert no hashea a la fingerprint esperada **falla en el handshake TLS**, no después.
- [ ] El probe `/info` pre-envío en `send_text` (lib.rs ~146) y `send_files` (lib.rs ~196) se elimina o se convierte en redundante (la pin lo cubre); ya no existe una ventana entre "verifico fingerprint por /info" y "envío el payload".
- [ ] `tauri.conf.json` tiene una `security.csp` estricta; la app arranca, muestra peers, envía texto/archivos y abre todos los modales sin errores de CSP en la consola del WebView.
- [ ] El thumbnail entrante, `senderIp`/`senderPort` y el SVG del QR ya no se inyectan por `innerHTML` con datos sin validar. Un peer que manda `name` o `thumbnail` malicioso no puede ejecutar JS.
- [ ] `POST /text` responde `403 FORBIDDEN` si el emisor no está habilitado por el mismo gate de consentimiento que `/clipboard`... **salvo** que decidamos que texto puntual no requiere opt-in (ver Tarea 3.4 y open questions). El comportamiento elegido queda documentado en el código.
- [ ] `POST /clipboard/image` y `POST /prepare-upload` tienen un `DefaultBodyLimit` explícito; un body de 200 MB a `/clipboard/image` se rechaza con `413` sin bufferizar 200 MB en RAM.
- [ ] El updater verifica un SHA-256 (o firma ed25519/minisign) del binario descargado **antes** de escribir el `.bat`/APK de swap. Un hash que no matchea aborta el update.
- [ ] `safe_join` rechaza `CON`, `PRN`, `AUX`, `NUL`, `COM1`..`COM9`, `LPT1`..`LPT9`, nombres con `:` (ADS), y nombres con `.`/espacio final.
- [ ] `cargo build` (desktop) y `cargo ndk`/build Android compilan; los tests unitarios nuevos pasan (ver "Cómo verificar").

## Tareas

### Tarea 3.1 — Cert pinning real por fingerprint (reemplaza `danger_accept_invalid_certs`)

**Problema.** El cliente acepta cualquier cert TLS y confía en un probe `/info` previo que devuelve la fingerprint esperada. Un atacante en el path responde el `/info` con la fingerprint correcta (la sabe: viaja en el TXT de mDNS y en el QR) y luego sirve el `/text`, `/upload`, `/clipboard` desde su propio cert. La pin nunca ocurre a nivel transporte, así que el MITM es transparente.

**Archivo(s).** `src-tauri/src/http_client.rs:29-40` (función `client()`), y todos los call-sites que hoy no pasan fingerprint.

**Estado actual.**
```rust
fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .pool_max_idle_per_host(8)
            .timeout(Duration::from_secs(300))
            .build()
            .expect("build reqwest client")
    })
}
```

Los call-sites conocen la fingerprint esperada en casi todos los flujos:
- `send_text` / `send_files` (lib.rs 129-201): tienen `peer_id` == fingerprint.
- `post_clipboard` / `post_clipboard_image` (lib.rs 831-859): iteran peers habilitados, tienen `fp`... del *emisor*, no del receptor — **cuidado, ver abajo**.
- `fetch_info` en el poller (discovery.rs 395) y en `add_peer_by_ip` (lib.rs 530) / `pair_with_qr_payload` (lib.rs 474): TOFU, todavía no confían en ninguna fingerprint; usan la respuesta para *descubrir* o *comparar*.

**Cambio.**

1. **Habilitar la construcción de un verifier custom.** En `Cargo.toml`, agregar la feature `danger_configuration` a rustls (necesaria para instalar un `ServerCertVerifier` propio):

```toml
rustls = { version = "0.23", features = ["ring", "danger_configuration"] }
```

2. **Implementar el verifier** en `http_client.rs`. La idea: NO validar cadena/CN/SAN (los peers son self-signed y se identifican por fingerprint), sino computar `SHA-256(DER end-entity)` y exigir que sea igual a la fingerprint esperada (misma definición que `identity.rs:63-67`).

```rust
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};

/// Verifier que pin-ea la fingerprint SHA-256 del cert end-entity.
/// Si `expected` es None, corre en modo TOFU: acepta cualquier cert
/// (equivalente al comportamiento viejo) — usado SOLO para /info de
/// descubrimiento donde todavía no confiamos en nadie.
#[derive(Debug)]
struct PinnedFingerprintVerifier {
    expected: Option<String>, // hex lowercase, igual a identity.fingerprint
}

impl ServerCertVerifier for PinnedFingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let mut hasher = Sha256::new();
        hasher.update(end_entity.as_ref());
        let got = hex::encode(hasher.finalize());
        match &self.expected {
            None => Ok(ServerCertVerified::assertion()), // TOFU
            Some(exp) if exp.eq_ignore_ascii_case(&got) => {
                Ok(ServerCertVerified::assertion())
            }
            Some(exp) => Err(TlsError::General(format!(
                "cert fingerprint mismatch: expected {}, got {}",
                &exp[..16.min(exp.len())],
                &got[..16]
            ))),
        }
    }

    // Peers usan cert self-signed sin CA; no validamos la firma del
    // handshake contra una raíz, pero rustls exige que estos métodos
    // existan. Devolvemos "válido" porque el pinning del cert ya nos
    // ata a la identidad correcta. NO copiar esto a un cliente que
    // hable con un servidor de CA pública.
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // ring soporta estos; devolver la lista estándar evita que
        // rustls aborte por "no schemes".
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}
```

3. **Construir un `reqwest::Client` por fingerprint esperada, cacheado.** Como cada peer tiene su propia fingerprint, y `use_preconfigured_tls` congela el `ClientConfig` (y por ende el verifier) al momento de `build()`, necesitamos un client por-peer. Mantené el pooling (crítico: comentario en http_client.rs:7-9 sobre LocalSend #1657) usando un cache `HashMap<String, reqwest::Client>` protegido por `Mutex`, keyeado por la fingerprint (o `"__tofu__"` para el modo descubrimiento).

```rust
fn build_client(expected_fp: Option<&str>) -> reqwest::Client {
    let verifier = Arc::new(PinnedFingerprintVerifier {
        expected: expected_fp.map(|s| s.to_ascii_lowercase()),
    });

    // Reusar el crypto provider de ring que ya forzamos en Cargo.toml.
    let mut cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    // ALPN vacío está bien: axum-server no negocia h2 acá.
    cfg.alpn_protocols.clear();

    reqwest::Client::builder()
        .use_preconfigured_tls(cfg)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .timeout(Duration::from_secs(300))
        .build()
        .expect("build reqwest client")
}

/// Devuelve un client pin-eado a `expected_fp`, creándolo una sola vez.
fn client_for(expected_fp: Option<&str>) -> reqwest::Client {
    static CACHE: OnceLock<Mutex<HashMap<String, reqwest::Client>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = expected_fp
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "__tofu__".to_string());
    let mut map = cache.lock().unwrap();
    map.entry(key)
        .or_insert_with(|| build_client(expected_fp))
        .clone()
}
```

`reqwest::Client` es `Clone` barato (es `Arc` internamente) y comparte el pool, así que clonar por request NO reabre conexiones. Import necesario arriba: `use std::sync::Mutex;` (ya está `OnceLock`; `HashMap` ya se importa en la línea 14).

4. **Cambiar las firmas de las funciones públicas para recibir la fingerprint esperada** y llamar `client_for(...)` en vez de `client()`:
   - `fetch_info(ip, port)` → **dos usos**: en el poller de discovery.rs se conoce `fp` (la clave del `by_fp`), y en `add_peer_by_ip`/`pair_with_qr_payload` NO. Solución mínima: agregar `fetch_info_pinned(ip, port, expected_fp: &str)` que usa `client_for(Some(expected_fp))` para el poller, y dejar `fetch_info(ip, port)` usando `client_for(None)` (TOFU) para descubrimiento/pairing. En el poller (discovery.rs:395) cambiar la llamada a `fetch_info_pinned(&ip, port, &fp)`.
   - `post_text` / `prepare_upload` / `upload_file` / `fetch_upload_progress` / `cancel_upload`: agregar parámetro `expected_fp: &str` y usar `client_for(Some(expected_fp))`. Los call-sites en lib.rs ya tienen `peer_id` (== fingerprint) disponible.
   - `post_clipboard` / `post_clipboard_image`: **el `fp` que hoy pasan es el del EMISOR, no el del receptor.** El receptor es el peer en `(ip, port)`. Hay que pasar la fingerprint del *destino*. En lib.rs:823 el filter arma `(ip, port)` desde `p.get(&fp)` — cambiar para arrastrar también la fingerprint del receptor (la clave `fp` de ese `filter_map` ya ES la del receptor; renombrar para no confundir con `my_fingerprint`). Pasar esa a `client_for(Some(receiver_fp))`.

5. **Eliminar el probe pre-envío spoofeable.** En `send_text` (lib.rs:146-155) y `send_files` (lib.rs:196-201), borrar el bloque `fetch_info(...)` + `if remote.fingerprint != peer_id`. La pin del transporte ya garantiza que el server que responde es el dueño de esa fingerprint; el probe extra solo agrega latencia y una ventana de TOCTOU.

**Estado actual del probe a eliminar (send_text):**
```rust
    let remote = http_client::fetch_info(&target.ip, target.port)
        .await
        .map_err(|e| format!("identity probe failed: {e:#}"))?;
    if remote.fingerprint != peer_id {
        return Err(format!(
            "fingerprint mismatch — expected {}, got {}",
            &peer_id[..16],
            &remote.fingerprint[..16]
        ));
    }
```

**Cambio:** reemplazar todo ese bloque por nada; `post_text(&target.ip, target.port, text, ..., /* expected_fp */ &peer_id)` ya pin-ea. Idéntico en `send_files` antes de `prepare_upload`.

**Por qué.** El pinning a nivel TLS es la única forma de que la garantía "hablo con el dueño de esta fingerprint" cubra **el mismo socket** por el que viajan los bytes. El probe `/info` verifica una conexión distinta de la que transfiere, así que no prueba nada contra un MITM activo.

**Cuidado con.**
- **NO** perder el pooling. Si por error creás un `Client` nuevo por request, reintroducís LocalSend #1657 (7000 archivos chicos → 80 KB/s). El cache por-fingerprint lo evita.
- El `crypto provider` de rustls debe estar inicializado. Como el proyecto ya fuerza `rustls = { features = ["ring"] }` y axum-server lo usa, `ClientConfig::builder()` toma el provider por defecto (ring). Si aparece un panic "no process-level CryptoProvider", agregar `rustls::crypto::ring::default_provider().install_default().ok();` una sola vez en el arranque (lib.rs, antes de levantar el server).
- `use_preconfigured_tls` acepta `impl Any` pero en la práctica se le pasa un `rustls::ClientConfig`; requiere que reqwest tenga una feature rustls activa (ya está `rustls-tls`). No cambia el resto de features.
- El modo TOFU (`expected=None`) mantiene el hueco viejo SOLO para descubrimiento/pairing, donde por definición aún no confiás en el peer. Eso es aceptable: `add_peer_by_ip` y `pair_with_qr_payload` comparan la fingerprint devuelta contra la esperada (QR) o la guardan como nueva identidad — el pinning real ocurre en el PRÓXIMO envío. Documentar esto con un comentario.
- Android habla el mismo `http_client.rs`, así que este fix también protege las transferencias desde/hacia el teléfono. **Esta tarea es prerrequisito para poder confiar en Android.**

---

### Tarea 3.2 — Content-Security-Policy estricta

**Problema.** `tauri.conf.json` tiene `"csp": null` y `index.html` no trae meta CSP. El WebView ejecuta cualquier `<script>` inline, cualquier `innerHTML`, y carga recursos remotos sin restricción. Combinado con las inyecciones de la Tarea 3.3, un peer puede lograr XSS.

**Archivo(s).** `src-tauri/tauri.conf.json:24-26`; secundariamente `src/index.html`.

**Estado actual.**
```json
    "security": {
      "csp": null
    }
```

**Cambio.** Poner una CSP estricta en `security.csp`. Tauri la inyecta como header/meta en el WebView:

```json
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; font-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'"
    }
```

**Por qué.** `default-src 'self'` + `script-src 'self'` corta la ejecución de JS inyectado por `innerHTML` y de handlers inline. `img-src 'self' data:` permite los thumbnails (que son `data:image/...`, ver thumbnails.rs:75) sin abrir a URLs remotas. `object-src 'none'` y `base-uri 'self'` cierran vectores clásicos.

**Cuidado con — qué se rompe y cómo mitigarlo (verificar TODO esto tras el cambio):**
- **Google Fonts** (index.html:28-33) viola `default-src 'self'`. La CSP de arriba NO incluye `fonts.googleapis.com`/`fonts.gstatic.com`. **Acción requerida:** descargar Orbitron/Audiowide/Share Tech Mono/JetBrains Mono y auto-hospedarlos (poné los `.woff2` en `src/fonts/` y `@font-face` en `styles.css`), y borrar los `<link>` a Google Fonts. Auto-hospedar es lo correcto para una app LAN/offline igual. Si por tiempo se difiere, la alternativa temporal es agregar `https://fonts.googleapis.com` a `style-src` y `https://fonts.gstatic.com` a `font-src`, pero eso reintroduce fetch remoto y rompe uso offline — no recomendado; anotarlo como deuda.
- **Handlers `onclick` inline** en index.html:315, 343, 457 (`onclick="document.getElementById(...).hidden=true"`). `script-src 'self'` **bloquea** estos handlers inline. **Acción requerida:** convertirlos a `addEventListener` en `main.js`. Son 3 botones de cierre de modal (`#peer-details-close`, `#add-peer-cancel`, `#settings-close`). Buscar en index.html `onclick=` y migrarlos todos.
- **`<script>` inline en `<head>`** (index.html:6-25, el detector `is-mobile`). `script-src 'self'` lo bloquea. **Acción requerida:** mover ese bloque a un archivo `src/pre.js` y referenciarlo con `<script src="pre.js"></script>` antes de `styles.css`. (No usar `'unsafe-inline'` en `script-src` — anularía media CSP.)
- `style-src 'unsafe-inline'` se deja porque el código usa estilos inline abundantes (`style="..."` en index.html y `.style.x=` / `innerHTML` con `style=` en main.js). Quitarlo es un refactor grande fuera de scope; documentarlo como deuda de seguridad menor (los estilos inline no ejecutan JS).
- `withGlobalTauri: true` + IPC: Tauri v2 necesita poder hablar con el backend. `connect-src 'self' ipc: http://ipc.localhost` cubre el transporte IPC del WebView2 en Windows. Si el IPC falla tras el cambio, revisar la consola: el esquema exacto puede variar por versión de Tauri; ajustar `connect-src` a lo que pida el error.

---

### Tarea 3.3 — Escapar / construir DOM sin `innerHTML` con datos de peers

**Problema.** Tres puntos inyectan datos controlados por el peer directo en `innerHTML`. Aunque la CSP (3.2) mitiga la ejecución de `<script>`, sigue habiendo vectores (`<img onerror=>`, etc.) y la CSP no debe ser la única defensa.

**Archivo(s).** `src/main.js:971-979` (thumbnail), `src/main.js:989-995` (`senderIp`/`senderPort`), `src/main.js:1235` y `:1239` (SVG del QR / error).

**Estado actual (thumbnail, 971-979):**
```javascript
      const thumb = f.thumbnail
        ? `<img class="incoming-thumb" src="${f.thumbnail}" alt="" />`
        : `<span class="incoming-thumb incoming-thumb-empty">📄</span>`;
      li.innerHTML = `
        ${thumb}
        <span class="file-name">${escapeHtml(f.name)}</span>
        <span class="file-size">${formatBytes(f.size)}</span>
      `;
```

**Estado actual (first-contact, 989-995):**
```javascript
      b.innerHTML = `
        <div class="settings-label" style="...">FIRST CONTACT</div>
        <div style="...">
          <span class="mono" style="...">${payload.senderIp}:${payload.senderPort || 53319}</span>
          <button class="modal-btn small" id="firstcontact-save">+ SAVE PEER</button>
        </div>
      `;
```

**Estado actual (QR, 1234-1239):**
```javascript
      const data = await invoke('generate_pair_qr');
      qrCanvas.innerHTML = data.svg || '';
      ...
    } catch (err) {
      qrCanvas.innerHTML = `<div style="padding:24px;color:#ff4d6b">${err}</div>`;
```

**Cambio.**

1. **Thumbnail — construir el `<img>` con `createElement` y validar el `src`.** `f.thumbnail` viene del peer; aunque debería ser `data:image/jpeg;base64,...` (thumbnails.rs:75), un peer malicioso puede mandar `data:text/html,...` o un `javascript:`... Validar el prefijo antes de asignar:

```javascript
      let thumb;
      if (f.thumbnail && /^data:image\/(png|jpeg|gif|webp);base64,/.test(f.thumbnail)) {
        thumb = document.createElement('img');
        thumb.className = 'incoming-thumb';
        thumb.alt = '';
        thumb.src = f.thumbnail; // ya validado como data:image/...
      } else {
        thumb = document.createElement('span');
        thumb.className = 'incoming-thumb incoming-thumb-empty';
        thumb.textContent = '📄';
      }
      const nameEl = document.createElement('span');
      nameEl.className = 'file-name';
      nameEl.textContent = f.name;           // textContent, no escapeHtml+innerHTML
      const sizeEl = document.createElement('span');
      sizeEl.className = 'file-size';
      sizeEl.textContent = formatBytes(f.size);
      li.replaceChildren(thumb, nameEl, sizeEl);
```

2. **`senderIp`/`senderPort` — usar `textContent`.** Reconstruir el bloque first-contact con nodos y setear el texto del `<span>` con `textContent`:

```javascript
      // ... crear b, setear id/className como hoy ...
      const label = document.createElement('div');
      label.className = 'settings-label';
      label.style.cssText = 'color:var(--neon-magenta);text-shadow:0 0 6px var(--neon-magenta-glow);margin-bottom:6px';
      label.textContent = 'FIRST CONTACT';
      const row = document.createElement('div');
      row.style.cssText = 'display:flex;gap:6px;align-items:center;justify-content:space-between';
      const addr = document.createElement('span');
      addr.className = 'mono';
      addr.style.cssText = 'font-size:11px;color:var(--text-mute)';
      addr.textContent = `${payload.senderIp}:${payload.senderPort || 53319}`; // textContent
      const saveBtn = document.createElement('button');
      saveBtn.className = 'modal-btn small';
      saveBtn.id = 'firstcontact-save';
      saveBtn.textContent = '+ SAVE PEER';
      row.append(addr, saveBtn);
      b.replaceChildren(label, row);
```

3. **QR SVG — el SVG viene del backend local (confiable), pero el branch de error interpola `err` sin escapar.** El SVG generado en lib.rs:428-434 es seguro (lo produce el crate `qrcode` a partir de la payload local). Dejar `qrCanvas.innerHTML = data.svg` **es aceptable** porque el dato es local y no de un peer. PERO el branch catch (1239) mete `${err}` en innerHTML — un mensaje de error puede contener texto arbitrario. Cambiarlo a texto:

```javascript
    } catch (err) {
      qrCanvas.replaceChildren();
      const e = document.createElement('div');
      e.style.cssText = 'padding:24px;color:#ff4d6b';
      e.textContent = String(err);
      qrCanvas.appendChild(e);
      qrCurrentPayload = '';
    }
```

Si se prefiere blindar también el SVG (defensa en profundidad), reemplazar `qrCanvas.innerHTML = data.svg` por img-encoding: `const img = new Image(); img.src = 'data:image/svg+xml;utf8,' + encodeURIComponent(data.svg); qrCanvas.replaceChildren(img);`. Opcional — el SVG es local.

**Por qué.** `textContent` no parsea HTML: elimina el vector por construcción. `createElement` + validación de `data:image/` evita que `src` sea un `javascript:`/`data:text/html`. El helper `escapeHtml` (main.js:733) escapa correctamente, pero mezclarlo con template strings + `innerHTML` es frágil; `textContent`/`createElement` es más robusto.

**Cuidado con.** No romper el listener de `#firstcontact-save` (main.js:997): sigue funcionando porque el `id` se mantiene y el `querySelector` posterior encuentra el botón. Mantené las mismas clases CSS para no alterar el look. `replaceChildren` es estándar en WebView2 y en el WebView de Android moderno.

---

### Tarea 3.4 — Gate de consentimiento en `/text` (igual que `/clipboard`)

**Problema.** `handle_clipboard` (http_server.rs:777) y `handle_clipboard_image` (http_server.rs:850) rechazan con `403` a peers no habilitados (`state.clipboard.is_enabled(...)`). `handle_text` (http_server.rs:191) **no tiene ningún gate**: cualquier peer del LAN puede empujar texto arbitrario que la UI muestra como "incoming-text". Esto permite spam/phishing desde cualquier dispositivo de la red.

**Archivo(s).** `src-tauri/src/http_server.rs:191-213`.

**Estado actual.**
```rust
async fn handle_text(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<TextPayload>,
) -> StatusCode {
    let sender_port = payload.sender_port.unwrap_or(crate::discovery::DEFAULT_PORT);
    let evt = IncomingTextEvent {
        text: payload.text,
        sender_alias: payload.sender_alias,
        sender_fingerprint: payload.sender_fingerprint,
        sender_ip: addr.ip().to_string(),
        sender_port,
        received_at: unix_now(),
    };
    ...
    let _ = state.app.emit("incoming-text", &evt);
    StatusCode::OK
}
```

**Cambio.** Decidir la política (ver open questions) e implementarla. **Opción recomendada — texto puntual NO requiere opt-in de clipboard-sync, pero SÍ requiere que el emisor sea un peer conocido/favorito, y en todo caso la UI ya muestra un toast** (`incoming-text` no auto-copia al clipboard como sí hace `/clipboard`). Si el equipo quiere paridad estricta con clipboard, gatear igual:

```rust
async fn handle_text(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<TextPayload>,
) -> StatusCode {
    // Consent gate — paridad con /clipboard: solo aceptamos texto de
    // peers con los que optamos por sincronizar. Ajustar la política
    // si texto puntual debe permitirse de cualquier peer conocido.
    if !state.clipboard.is_enabled(&payload.sender_fingerprint) {
        return StatusCode::FORBIDDEN;
    }
    // ... resto igual ...
}
```

**Por qué.** Sin gate, el endpoint `/text` es un canal de mensajería no solicitada abierto a todo el LAN. `/clipboard` ya trata esto como un problema; `/text` debe ser al menos tan estricto.

**Cuidado con.**
- `is_enabled` chequea el **clipboard-sync opt-in**, que semánticamente es "sincronizá mi portapapeles con este peer", no "aceptá texto puntual". Si el producto quiere que enviar texto a un peer sea posible sin activar clipboard-sync, este gate rompe ese flujo (el receptor tendría que habilitar sync primero). **Confirmar la política antes de mergear** — de ahí que esté en open questions. Una alternativa es un gate más laxo: aceptar si el emisor es favorito O está en la lista de peers conocidos, en vez de `is_enabled`. En ese caso hace falta acceso a `prefs`/`discovery` desde el handler (ya está `state.prefs`).
- El cliente `post_text` (http_client.rs:97) ya trata `403` como error de envío; el emisor verá "send failed". Si se gatea, el UX del emisor debería explicar "el receptor no te tiene habilitado". No es bloqueante para esta fase.

---

### Tarea 3.5 — `DefaultBodyLimit` en rutas de imagen/upload

**Problema.** `handle_clipboard_image` (http_server.rs:843) recibe el PNG como base64 dentro de un `Json<ClipboardImagePayload>`. Axum bufferiza **todo** el body en memoria para deserializar el JSON. El chequeo `bytes.len() > 32*1024*1024` (línea 857) ocurre **después** de decodificar base64 — para entonces ya bufferizaste el JSON completo en RAM. Peor: el `DefaultBodyLimit` global de axum es 2 MB, así que hoy imágenes legítimas >~1.5 MB de PNG fallan silenciosamente con `413` **antes** de llegar al handler, y el `router` en `run()` (http_server.rs:128-140) **no** setea ningún límite explícito ni por-ruta. Resultado: o rechazás imágenes válidas (default 2 MB) o, si alguien sube el default, un atacante manda 500 MB de JSON y te agota la RAM.

**Archivo(s).** `src-tauri/src/http_server.rs:128-140` (definición del `router` en `run`).

**Estado actual.**
```rust
    let router = Router::new()
        .route("/info", get(handle_info))
        .route("/text", post(handle_text))
        .route("/prepare-upload", post(handle_prepare_upload))
        .route("/upload/{session_id}/{file_id}", post(handle_upload))
        .route(
            "/upload/{session_id}/{file_id}/progress",
            get(handle_upload_progress),
        )
        .route("/cancel/{session_id}", post(handle_cancel))
        .route("/clipboard", post(handle_clipboard))
        .route("/clipboard/image", post(handle_clipboard_image))
        .with_state(state);
```

**Cambio.** Aplicar un `DefaultBodyLimit` por-ruta a las rutas que bufferizan JSON grande. Importar `use axum::extract::DefaultBodyLimit;`. Base64 infla ~4/3, así que para permitir un PNG de 32 MB hay que permitir ~44 MB de JSON; redondeá a 48 MB. `/upload` NO necesita subir el límite porque hoy ya **streamea** el body (usa `Body`/`StreamExt`, no `Json`), pero SÍ conviene un límite razonable en `/prepare-upload` (que sí bufferiza JSON con thumbnails base64).

```rust
    use axum::extract::DefaultBodyLimit;

    // Base64 infla ~4/3; 48 MiB de JSON deja pasar un PNG de ~32 MiB.
    const CLIP_IMAGE_LIMIT: usize = 48 * 1024 * 1024;
    // prepare-upload lleva N thumbnails base64 (~64x64 c/u) + metadata;
    // 8 MiB es holgado y corta payloads absurdos.
    const PREPARE_LIMIT: usize = 8 * 1024 * 1024;

    let router = Router::new()
        .route("/info", get(handle_info))
        .route("/text", post(handle_text))
        .route(
            "/prepare-upload",
            post(handle_prepare_upload).layer(DefaultBodyLimit::max(PREPARE_LIMIT)),
        )
        .route("/upload/{session_id}/{file_id}", post(handle_upload))
        .route(
            "/upload/{session_id}/{file_id}/progress",
            get(handle_upload_progress),
        )
        .route("/cancel/{session_id}", post(handle_cancel))
        .route("/clipboard", post(handle_clipboard))
        .route(
            "/clipboard/image",
            post(handle_clipboard_image).layer(DefaultBodyLimit::max(CLIP_IMAGE_LIMIT)),
        )
        .with_state(state);
```

Además, alinear el chequeo interno con el límite: en http_server.rs:857 el `bytes.len() > 32 * 1024 * 1024` sigue siendo válido como segunda barrera (rechaza el PNG *decodificado* >32 MB aun si el JSON entró); mantenerlo.

**Por qué.** El `layer(DefaultBodyLimit::max(...))` por-ruta hace que axum aborte con `413` **antes** de bufferizar más allá del límite, sin tocar el límite global de las otras rutas. Así `/clipboard/image` acepta imágenes legítimas grandes pero corta un DoS de RAM, y `/text`/`/clipboard`/`/info` mantienen el default chico (2 MB) que les sobra.

**Cuidado con.**
- No subir el límite **global** (`Router::layer(DefaultBodyLimit::max(...))` sin `.route(...)`), porque abriría `/text` a bodies enormes también. Aplicar SIEMPRE por-ruta.
- `/upload` streamea (no `Json`); NO le pongas un límite chico o cortás archivos grandes. Su tamaño ya se acota por `content-length`/`size` en la sesión. Dejar `/upload` sin `DefaultBodyLimit` (o con uno enorme si el equipo prefiere ser explícito).
- (Opcional, mejora mayor fuera de scope) mover la imagen de clipboard a un body raw streamed en vez de JSON base64 eliminaría el inflado 4/3 y el buffer completo. Es un cambio de protocolo (romper compat con clientes viejos) — anotarlo como deuda; NO hacerlo en esta fase.

---

### Tarea 3.6 — Verificar integridad del binario del updater antes de stagear

**Problema.** El updater (updater.rs) descarga el `.exe`/APK de GitHub y lo stagea sin verificar **nada** (comentario explícito en updater.rs:7-9). Si la conexión a `api.github.com`/`objects.githubusercontent.com` es interceptada (o el mirror es comprometido), se ejecuta un binario arbitrario con los privilegios del usuario. El `download_and_stage` (Windows, línea 133) y `download_and_stage_apk` (Android, línea 202) escriben el binario y arman el swap sin chequear hash/firma.

**Archivo(s).** `src-tauri/src/updater.rs:133-179` (Windows) y `:202-225` (Android); `check_for_update` (:53) para propagar el hash esperado.

**Estado actual (Windows stage, resumido):**
```rust
pub async fn download_and_stage(download_url: &str) -> Result<()> {
    let client = reqwest::Client::builder() ... .build()?;
    let bytes = client.get(download_url).send().await? ... .bytes().await?;
    let staged = temp_dir.join("millennium-clipboard-update.exe");
    tokio::fs::write(&staged, &bytes).await?;
    // ... escribe .bat de swap y lo lanza ...
}
```

**Cambio (enfoque recomendado: SHA-256 publicado en el body del release).** Es el de menor fricción operativa: no requiere gestionar una clave privada. El release en GitHub ya expone `body` (release notes, updater.rs:87). Publicá en el body una línea con el SHA-256 del asset, p. ej.:

```
sha256:9f2b...e7  Millennium Clipboard.exe
```

1. En `check_for_update`, extraer del `release_notes` el SHA-256 esperado y agregarlo a `UpdateInfo`:

```rust
pub struct UpdateInfo {
    // ... campos existentes ...
    pub download_sha256: Option<String>, // hex lowercase, del body del release
}

fn extract_sha256(body: &str) -> Option<String> {
    // Busca "sha256:<64 hex>" (case-insensitive) en el body del release.
    for tok in body.split(|c: char| c.is_whitespace() || c == ':') {
        let t = tok.trim();
        if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(t.to_ascii_lowercase());
        }
    }
    None
}
```

Llenar `download_sha256: extract_sha256(&release_notes)` en el `Ok(UpdateInfo { ... })`.

2. Pasar el hash esperado a `download_and_stage`/`download_and_stage_apk` y verificar tras descargar, antes de escribir el swap:

```rust
pub async fn download_and_stage(download_url: &str, expected_sha256: Option<&str>) -> Result<()> {
    // ... descarga bytes como hoy ...

    // Verificación de integridad — abortar si no matchea.
    let expected = expected_sha256.ok_or_else(|| {
        anyhow::anyhow!("release no publicó SHA-256; abortando update por seguridad")
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let got = hex::encode(hasher.finalize());
    if !got.eq_ignore_ascii_case(expected) {
        bail!(
            "checksum mismatch: esperado {}, obtenido {} — NO se instala",
            &expected[..16], &got[..16]
        );
    }
    crate::runtime_log::info(format!("[updater] SHA-256 verificado OK ({})", &got[..16]));

    // ... recién ahora escribir staged + .bat como hoy ...
}
```

Import: `use sha2::{Digest, Sha256};` (sha2 ya es dependencia). Actualizar los call-sites del comando Tauri que invocan `download_and_stage(...)` para pasar `update_info.download_sha256.as_deref()`. Igual patrón para `download_and_stage_apk`.

**Alternativa más fuerte (firma ed25519/minisign).** Si el equipo prefiere firma criptográfica (resiste un GitHub comprometido, no solo MITM del transporte): usar el crate `minisign-verify` (verificación pura, sin dependencias de red) con una clave pública **embebida en el binario** (`const MINISIGN_PUBKEY: &str = "..."`). El release publica `<asset>.minisig`; el updater descarga el `.minisig`, y llama `minisign_verify::PublicKey::from_base64(MINISIGN_PUBKEY)?.verify(&bytes, &sig, false)?` antes de stagear. Esto es estrictamente mejor que el SHA-256-en-body (el body es editable por quien controle el release; una firma requiere la clave privada offline). Recomendado si hay tiempo; el SHA-256 es el mínimo aceptable.

**Por qué.** Ejecutar un binario descargado sin verificar integridad es RCE por diseño ante cualquier compromiso del canal de distribución. Un hash publicado corta el MITM del transporte; una firma corta también el compromiso del repositorio.

**Cuidado con.**
- Si `download_sha256` es `None` (release viejo sin hash en el body), el enfoque de arriba **aborta el update**. Es el fail-safe correcto, pero rompe updates desde releases legacy. Decidir: (a) abortar (seguro, recomendado), o (b) permitir con un warning visible. Anotado en open questions.
- Windows: el `.bat` de swap (updater.rs:158) corre DESPUÉS de la verificación — bien. No stagear ni escribir el `.bat` si el hash falla (el `?`/`bail!` garantiza early-return).
- Android: `download_and_stage_apk` (línea 202) tiene la misma falta; aplicar idéntica verificación antes de `tokio::fs::write(&apk_path, ...)`. El instalador de Android verifica la firma APK del paquete, pero eso solo garantiza que el APK está firmado por *alguna* clave, no por la tuya — la verificación SHA-256/minisign sigue aportando.

---

### Tarea 3.7 — `safe_join`: rechazar nombres reservados de Windows, ADS y dots/espacios finales

**Problema.** `safe_join` (http_server.rs:981) sólo acepta `Component::Normal` y rechaza `..`/absolutos/prefijos de drive. Pero en Windows un `Component::Normal` puede ser `CON`, `NUL`, `COM1`, etc. (dispositivos reservados), puede contener `:` (Alternate Data Streams, p. ej. `foo.txt:evil`), o terminar en `.`/espacio (Windows los strippea, causando colisiones/escritura en un nombre distinto). Un peer malicioso manda `name = "CON"` o `name = "report.txt:hidden"` y el receptor abre/escribe algo inesperado.

**Archivo(s).** `src-tauri/src/http_server.rs:981-998`.

**Estado actual.**
```rust
fn safe_join(base: &Path, name: &str, rel_path: Option<&str>) -> Option<PathBuf> {
    let mut target = base.to_path_buf();
    if let Some(rel) = rel_path {
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(s) => target.push(s),
                _ => return None,
            }
        }
    }
    for comp in Path::new(name).components() {
        match comp {
            Component::Normal(s) => target.push(s),
            _ => return None,
        }
    }
    Some(target)
}
```

**Cambio.** Validar cada componente `Normal` con un helper que rechaza los casos peligrosos:

```rust
/// True si el componente es un nombre de archivo seguro para escribir en
/// disco (multiplataforma, con foco en las trampas de Windows).
fn is_safe_component(s: &std::ffi::OsStr) -> bool {
    let name = match s.to_str() {
        Some(n) => n,
        None => return false, // no UTF-8 válido → rechazar
    };
    if name.is_empty() {
        return false;
    }
    // ADS / drive-relative: cualquier ':' es sospechoso en Windows.
    if name.contains(':') {
        return false;
    }
    // Caracteres ilegales en NTFS (y peligrosos en general).
    if name.contains(['/', '\\', '<', '>', '"', '|', '?', '*']) {
        return false;
    }
    // Bytes de control.
    if name.chars().any(|c| (c as u32) < 0x20) {
        return false;
    }
    // Windows strippea '.' y espacios finales → colisión/escritura en
    // un nombre distinto del pedido.
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    // Nombres de dispositivo reservados (con o sin extensión):
    // CON, PRN, AUX, NUL, COM1..COM9, LPT1..LPT9.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return false;
    }
    true
}

fn safe_join(base: &Path, name: &str, rel_path: Option<&str>) -> Option<PathBuf> {
    let mut target = base.to_path_buf();
    if let Some(rel) = rel_path {
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(s) if is_safe_component(s) => target.push(s),
                _ => return None,
            }
        }
    }
    for comp in Path::new(name).components() {
        match comp {
            Component::Normal(s) if is_safe_component(s) => target.push(s),
            _ => return None,
        }
    }
    Some(target)
}
```

**Por qué.** El check actual bloquea traversal (`..`, absolutos) pero no las trampas Windows-específicas. Rechazar en el join es la barrera correcta: si `safe_join` devuelve `None`, el handler de upload ya trata eso como error y no escribe nada.

**Cuidado con.**
- No romper nombres legítimos con puntos internos (`report.final.pdf` está OK: solo se rechaza `.`/espacio *final* y el *stem* reservado). `is_safe_component` chequea `split('.').next()` → para `CON.txt` el stem es `CON` → rechazado (correcto, Windows lo trata como el dispositivo). Para `report.txt` el stem es `report` → OK.
- El helper corre para `rel_path` **y** `name`, así que también bloquea un subdirectorio llamado `NUL`.
- Es multiplataforma por simplicidad (en Linux `CON` sería un nombre válido, pero rechazarlo no daña nada y protege si el archivo luego se copia a un Windows). Si molesta, gatear el chequeo de RESERVED con `cfg!(windows)`; no recomendado — la portabilidad de los nombres recibidos importa.
- Confirmá que el call-site de `safe_join` en `handle_upload` ya maneja `None` como error (rechazo del archivo). Buscar `safe_join(` en http_server.rs y verificar el `?`/`match`.

## Cómo verificar

Comandos de build/run/test están en `../00-SHARED-CONTEXT.md`; acá van las verificaciones específicas.

1. **3.1 pinning — happy path.** Con dos instancias reales (o `MILLENNIUM_INSTANCE`/`MILLENNIUM_PORT` para dos procesos en la misma máquina, ver identity.rs:34 y discovery.rs:26), enviar texto y un archivo entre peers. Debe funcionar igual que antes. Confirmar en logs que NO reaparece el throughput colapsado (el pooling sigue vivo): transferir ~50 archivos chicos y ver que no baja a ~80 KB/s.
2. **3.1 pinning — ataque simulado.** Levantar un segundo server con OTRA identidad (otro cert) escuchando en el `ip:port` que la UI cree que es el peer bueno (o cambiar a mano la fingerprint esperada). El envío debe fallar en el **handshake TLS** con "cert fingerprint mismatch", no después. Test unitario: en `http_client.rs`, `#[test]` que instancia `PinnedFingerprintVerifier { expected: Some("aa..") }` y llama `verify_server_cert` con un `CertificateDer` cuyo SHA-256 ≠ `aa..`; assert que devuelve `Err`. Y otro con la fingerprint correcta → `Ok`.
3. **3.2 CSP.** Abrir la app, F12/devtools del WebView (o los logs de WebView2), y confirmar CERO violaciones de CSP al: arrancar, escanear peers, abrir Settings, abrir QR, recibir texto/archivos. Verificar que las fuentes cargan (auto-hospedadas) y que los 3 botones de cierre de modal (antes `onclick=`) funcionan.
4. **3.3 escaping.** Enviar desde un peer un archivo con `name = "<img src=x onerror=alert(1)>"`. El modal de incoming debe mostrar el texto literal, sin ejecutar nada ni romper el layout. Enviar un `thumbnail` que NO empiece con `data:image/` → debe caer al icono `📄`.
5. **3.4 /text gate.** Con clipboard-sync DESHABILITADO para el emisor, `POST /text` debe responder `403`. Con la política elegida documentada, confirmar que el flujo esperado (favorito / opt-in) sí pasa.
6. **3.5 body limit.** `curl -k -X POST https://127.0.0.1:PORT/clipboard/image -H 'content-type: application/json' --data-binary @big.json` con un JSON >48 MB → `413`. Un PNG legítimo de ~5 MB (que antes fallaba por el default de 2 MB) → ahora entra. Observar RAM del proceso en el Administrador de tareas: no debe spikear a cientos de MB por el request grande.
7. **3.6 updater.** Test unitario de `extract_sha256`: body `"...\nsha256:<64 hex>\n..."` → `Some(hex)`; body sin hash → `None`. Prueba manual: corromper un byte del `.exe` en un mirror local y confirmar que `download_and_stage` aborta con "checksum mismatch" y NO escribe el `.bat`.
8. **3.7 safe_join.** Test unitario en http_server.rs: `assert!(safe_join(base, "CON", None).is_none())`, `"NUL.txt"` → `None`, `"a:b"` → `None`, `"trailing "` → `None`, `"trailing."` → `None`, `"report.final.pdf"` → `Some(...)`, `"normal.txt"` → `Some(...)`. Y con `rel_path = Some("sub/NUL")` → `None`.

## Riesgo y rollback

- **3.1 es el cambio de mayor riesgo:** toca el transporte de TODO envío. Si el verifier o el crypto provider quedan mal, ningún envío funciona. Mitigación: shippear 3.1 solo/a con feature-flag mental (probar happy path exhaustivamente antes de mergear). **Rollback:** revertir `http_client.rs` a `client()` con `danger_accept_invalid_certs(true)` y restaurar el probe `/info` en lib.rs. Estos son cambios contenidos en 2 archivos.
- **3.2 CSP** puede "romper" la UI si algún recurso queda fuera de la policy (fuentes, IPC, un inline handler olvidado). Es visible de inmediato en la consola. **Rollback:** `"csp": null`. Shippeable independientemente del resto, PERO depende de 3.3 y de migrar fuentes/handlers/inline-script para no romper la app — hacé 3.2+3.3+migraciones juntas.
- **3.3, 3.4, 3.5, 3.7** son de bajo riesgo y **shippeables independientemente**. 3.3 es puramente frontend. 3.4 puede afectar UX (ver open questions) — es el único con impacto funcional. 3.5 y 3.7 son endurecimientos server-side sin cambio de contrato visible (salvo rechazar payloads/nombres maliciosos que antes pasaban).
- **3.6 updater** es independiente y de bajo riesgo salvo el fail-safe de releases legacy (ver open questions). **Rollback:** dejar de exigir el hash (pasar `None` sin abortar). No afecta el resto de la app.
- **Orden de merge sugerido:** 3.7 → 3.5 → 3.4 → 3.6 (independientes, fáciles) → 3.3 → 3.2 (juntas) → 3.1 (última, más test). 3.1 no depende de las otras pero conviene aislarla para bisectar si algo falla.
