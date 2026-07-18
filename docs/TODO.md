# TODO — Millennium Clipboard

> ÚNICA fuente de pendientes del proyecto. Completado → SE BORRA (la historia vive en CHANGELOG y git). Header de 1 línea, sin narrativa de sesión.

2026-07-18 — ver SESSION_HANDOFF.md

## 🔵 Displays (SPEC-displays — misión activa; roadmap de fases en `docs/SPEC-displays.md`)
- [ ] **Fase 0 — smoke en la máquina de 3 displays** (cierra la Fase 0 del todo): bajar el `.exe` del [run verde](https://github.com/guidocameraeq/Millennium-Clipboard/actions/runs/29650684956), correrlo, ver `[displays] Fase 0 link smoke: … status=0 paths=N modes=M` en el log + confirmar que clipboard/discovery/transferencias siguen igual. **Es lo único físico pendiente de Fase 0** (el CI ya la dio por verde).
- [ ] **Fase 1 — Ver los monitores (siguiente build)**: vendorizar el crate puro `monarch` por subtree + `displays_get_snapshot` read-only + HUD/modal/lista. Detalle en HANDOFF y en el SPEC. Chat nuevo.
- [ ] **Sumar el build de Android al CI** (follow-up, no bloquea Fase 1): `cargo check` corre en el host y NO caza una fuga de `cfg` que rompa Android — solo `tauri android build` la revela. Guard automático antes de sumar más código Win32.

## 🔴 Crítico
- [ ] **Fase 2 — verificación física Bloque B (UI): faltan 4** (necesitan 2 PCs). Bloque A (datos) ✅ verificado 2026-07-15 (ver CHANGELOG). Faltan: **TARGET LOST**, **error que no se pisa a los 5 s**, **barras TX/RX independientes**, **rename que sobrevive un `peers-changed`**. Notas: en una misma PC NO corren 2 instancias (single-instance por identifier) → 2 PCs, o cerrar la real + 1 instancia aislada (`MILLENNIUM_INSTANCE`+`MILLENNIUM_PORT`). Para TARGET LOST hace falta un peer **NO favorito** (`DRACOSSSLAPTOP` es favorito; `PEER_TTL=15 s`).
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
- [ ] **UI-polish — round-trip físico de transferencia (criterio #1)**: falta enviar/recibir texto y archivo entre 2 PCs con el frontend nuevo (acá no se pudo: single-instance bloquea un 2º peer local; peers reales offline). Riesgo casi nulo — solo cambió UI, el motor de transferencia está intacto. Eyeball en la próxima sesión de 2 PCs.
- [ ] **Fase 3 — sub-checks opcionales no corridos en vivo** (el core SÍ se verificó el 2026-07-14: auto-update en las 2 PCs + transferencias bidireccionales OK → pinning no rompe el uso diario, CSP no rompe la app). Faltan, sin urgencia: el **ataque simulado** (2º server con otro cert en el `ip:port` del peer bueno → debe fallar el handshake) —ya probado por máquina con el harness de handshake real (cert copiado → `BadSignature`), falta la prueba física—; el bulk de ~50 archivos chicos (throughput/pooling); y F12 sin violaciones de CSP de forma explícita.
- [ ] **UI — zonas protegidas (diferido del SPEC-ui-polish, decisión D3)**: (a) el conteo de peers aparece repetido 3-4 veces (badge + "NN visible" + PEERS/FAV del pie); (b) la lista de peers no se navega con teclado. Ambos tocan el render por diff (`renderPeers`/`buildPeerItem`) → requieren su propio spec chico + OK para entrar a la zona protegida.
- [ ] **UI — aviso visual cuando `prefs` se corrompe**: hoy la corrupción de favoritos solo deja rastro en el log + `.corrupt`, sin cartel en pantalla (`settings` sí tiene manejo especial). Mejora de UX chica; detectada en la verificación física de Fase 2 (2026-07-15).
- [ ] Suite de tests real (hoy no hay). Que cada fase que lo pida agregue tests unitarios Rust.
