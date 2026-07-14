# TODO — Millennium Clipboard

> ÚNICA fuente de pendientes del proyecto. Completado → SE BORRA (la historia vive en CHANGELOG y git). Header de 1 línea, sin narrativa de sesión.

2026-07-14 — ver SESSION_HANDOFF.md

## 🔴 Crítico
- [ ] **Fase 2 — verificación FÍSICA del usuario** (implementada + review, pero NO probada en vivo). Datos: agregar un favorito → matar el proceso a mitad → reabrir (debe seguir); corromper un JSON a mano → reabrir (favoritos a default PERO aparece `<archivo>.json.corrupt` + `ERR [jsonstore] parse failed`). UI (2 instancias con `MILLENNIUM_INSTANCE` o 2 PCs): TARGET LOST, error que no se pisa a los 5 s, texto entrante que sobrevive un ACK, barras TX/RX independientes, rename que sobrevive un `peers-changed`.
- [ ] **DECIDIR (antes de tocar Android):** núcleo headless vs foreground-only (`android/SPEC.md`)

## 🟠 Seguridad (fuera de fase, chico)
- [ ] **Autostart sin comillas (CWE-428)**: la entrada de autostart (`HKCU\...\Run`) que escribe `tauri-plugin-autostart` no lleva comillas → *unquoted path* con rutas con espacios. Hoy funciona por la heurística de Windows, pero conviene reescribirla con comillas. (Estaba anotado dentro de la línea de Fase 3; NO se tocó en esa fase — el plugin controla el quoting, hay que post-procesar la entrada del registro.)

## 🟡 Cuando se pueda
- [ ] **Sin autenticación mutua del cliente (no mTLS)** — el server HTTPS usa `with_no_client_auth`, así que NO verifica la identidad de quien envía. Cualquier gate por `sender_fingerprint` (el de `/clipboard`, y el de `/text` si algún día se cierra) es spoofeable por quien conozca una huella conocida (viaja en la TXT de mDNS/QR). El cert pinning de Fase 3 (Tarea 3.1) protege al EMISOR (pin del receptor), no al receptor contra un emisor falso. Cerrarlo = client-cert pinning bidireccional (mTLS): cambio de handshake en ambos lados + compat con peers viejos. Grande; no urgente (la app es solo-LAN). Anotado desde Fase 3.
- [ ] **Zombie-killer mata una instancia SANA en doble-launch** (pre-existente, NO regresión de Fase 2; confirmado por el review). El binario ya se llama `millennium-clipboard.exe`, así que el killer siempre mató a la instancia viva al relanzar, defeateando el "enfocar ventana" de single-instance. Hoy tolerable (el estado está persistido y se recarga). Fix correcto: chequear liveness (probe HTTPS `/info`) antes de matar — solo matar al que NO responde (el zombie real). Es más grande; no urgente.
- [ ] **Fragilidad del harness de test en Windows**: agregar tests al crate rompe la carga del binario de test del lib (comctl32-v6 sin manifest → `STATUS_ENTRYPOINT_NOT_FOUND`). Por eso los tests de `json_store` van `#[cfg(not(windows))]` + harness aislado. Si alguna vez se quiere `cargo test` con GUI-tests en Windows, embeber el manifest en los binarios de test (linker arg) o extraer la lógica testeable a un crate sin Tauri.
- [ ] Fase 1 — probar físicamente lo opcional: roaming (re-anuncio al cambiar de red) y QR con la IP nueva tras un roam. Verificado por máquina, no físico. No bloquea nada.
- [ ] Android Fase A — ciclo de vida + aprobación nativa (`android/phase-A-lifecycle-and-approval.md`)
- [ ] Android Fase B — binding WiFi + streaming a MediaStore (`android/phase-B-discovery-and-storage.md`)
- [ ] Android Fase C — portapapeles, QR, UI móvil (`android/phase-C-clipboard-qr-mobile.md`)

## 🟢 Ideas / algún día
- [ ] **Fase 3 — sub-checks opcionales no corridos en vivo** (el core SÍ se verificó el 2026-07-14: auto-update en las 2 PCs + transferencias bidireccionales OK → pinning no rompe el uso diario, CSP no rompe la app). Faltan, sin urgencia: el **ataque simulado** (2º server con otro cert en el `ip:port` del peer bueno → debe fallar el handshake) —ya probado por máquina con el harness de handshake real (cert copiado → `BadSignature`), falta la prueba física—; el bulk de ~50 archivos chicos (throughput/pooling); y F12 sin violaciones de CSP de forma explícita.
- [ ] Suite de tests real (hoy no hay). Que cada fase que lo pida agregue tests unitarios Rust.
