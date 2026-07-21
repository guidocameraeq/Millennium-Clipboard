# TODO — Millennium Clipboard

> ÚNICA fuente de pendientes del proyecto. Completado → SE BORRA (la historia vive en CHANGELOG y git). Header de 1 línea, sin narrativa de sesión.

2026-07-21 — ver SESSION_HANDOFF.md

## 🔵 Displays (SPEC-displays — misión activa; roadmap de fases en `docs/SPEC-displays.md`)
- [ ] **Fase 3 — sub-checks físicos que faltan** (el núcleo ya se verificó en hardware el 2026-07-21: perfiles, lienzo, auto-revert, updater — ver CHANGELOG). Faltan, de paso en el próximo uso: (a) **cambiar el plazo del auto-revert desde AJUSTES** y ver que el próximo cambio lo use; (b) **enchufar/desenchufar** algo y ver la LISTA actualizarse **sola, sin apretar REFRESH** (el watcher `WM_DISPLAYCHANGE`); (c) **regresión**: transferencia/clipboard siguen igual y **CPU en reposo ~0% en el Task Manager**. Con estos, el SPEC-displays queda COMPLETO y se archiva.
- [ ] **Si aparece "apagué la TV, cerré la app, la abrí y no la puedo prender"**: es el costo declarado de no portar la persistencia binaria del snapshot (ADR-008). La cura correcta es re-agregarla parseando campo por campo, **nunca** con el `assume_init` del donante.
- [ ] **El CI corre ante cualquier push a `feat/displays`, incluidos los de solo documentación** (6,5 min desperdiciados por cada `/cierre`). Agregar `paths-ignore: ['docs/**', '**.md']` al trigger de `.github/workflows/build.yml`. Chico; hacerlo de paso en la próxima sesión.
- [ ] **CPU en reposo tras la Fase 1**: no se verificó en Task Manager. El diff no agrega poll ni timer (la enumeración corre solo al abrir el modal o apretar REFRESH) ⇒ riesgo teórico, pero sin evidencia. Chequear de paso en la próxima corrida de la app.

## 🟣 Displays v2 — PRÓXIMA MISIÓN (backlog para el Arquitecto, 2026-07-21)

> Lo que Guido esperaba de la Fase 3 y no estaba, + features nuevas. **NO es un spec todavía**: es la
> materia prima. Arrancar la misión con el **Arquitecto (Modo B: feature grande sobre app que anda)** →
> explora el código real + entrevista → **SPEC delta** con su "qué NO se toca" antes de tocar una línea.
> Guido pidió resolver primero lo que ya funciona; la resolución-por-perfil es "para más adelante".

**Grupo 1 — el motor de Monarch YA lo soporta; falta cablearlo a la UI (barato):**
- [ ] **Elegir el monitor primario** al armar/editar un perfil. `OutputConfig.primary` ya existe en el
  modelo; falta UI (marcar primario en la lista o el lienzo) y que `save_profile` lo capture.
- [ ] **Shortcuts para aplicar un perfil** (importante para Guido). `AppSettings.profile_shortcuts` +
  `profile_shortcut_base` + `global_shortcuts_enabled` ya están en el modelo; falta registrar los
  hotkeys globales (Tauri: `tauri-plugin-global-shortcut`) + UI para asignarlos. El
  `global_shortcuts_enabled` NO es decoración: es el interruptor general de ese sistema — es la infra
  que los shortcuts necesitan, no algo a "sacar".
- [ ] **Aplicar un perfil al iniciar** (startup profile): al encender la PC aplica cierto perfil (caso de
  uso de Guido: dejó la TV prendida). Si ya está aplicado, no-op. `AppSettings.startup_profile_name` ya
  existe; falta wire (aplicar al arranque, no-op si coincide) + UI para elegirlo. (La app ya tiene
  `tauri-plugin-autostart`.)
- [ ] **[Más adelante] Resolución/refresh por perfil** (un perfil pone la TV en 1080p, otro en 4K).
  `OutputConfig.resolution`/`refresh_rate_mhz` ya se guardan y aplican; falta capturar/editar la
  resolución por perfil en la UI. No urgente (Guido lo dijo explícito).

**Grupo 2 — net-new, requiere INVESTIGACIÓN:**
- [ ] **Cambio de audio por perfil** (importante). Al aplicar un perfil, cambiar también el output de
  audio por default de Windows (ej. salida por la TV). **NO está en Monarch.** Investigar la API de
  Windows para setear el default audio device (IMMDevice/IPolicyConfig — ojo, `IPolicyConfig` es
  semi-documentada). Extender el perfil para guardar el "audio deseado" (es dato del usuario → cuidado
  con la migración del schema).

**Grupo 3 — rework de UI grande:**
- [ ] **Rediseño: displays deja de ser pop-up y pasa a módulo full-screen.** La app dividida en DOS
  grandes secciones/pestañas: (a) archivos + clipboard (lo actual), (b) displays con su propia pantalla
  completa. Toca el shell del frontend (`index.html`/`main.js`); **NO** el motor de transferencias ni el
  de displays. Es lo más caro; el Arquitecto tiene que scopear bien el "qué NO se toca".

**Grupo 4 — UX polish:**
- [ ] **Editar/actualizar un perfil sin borrar+recargar.** El overwrite YA existe (guardar con el mismo
  nombre → banner → reemplaza), pero no se descubre. Sumar un botón "actualizar este perfil con el
  layout actual" por fila, y quizás renombrar. Chico.

## 🔴 Crítico
- [ ] **Fase 2 — verificación física Bloque B (UI): faltan 4** (necesitan 2 PCs). Bloque A (datos) ✅ verificado 2026-07-15 (ver CHANGELOG). Faltan: **TARGET LOST**, **error que no se pisa a los 5 s**, **barras TX/RX independientes**, **rename que sobrevive un `peers-changed`**. Notas: en una misma PC NO corren 2 instancias (single-instance por identifier) → 2 PCs, o cerrar la real + 1 instancia aislada (`MILLENNIUM_INSTANCE`+`MILLENNIUM_PORT`). Para TARGET LOST hace falta un peer **NO favorito** (`DRACOSSSLAPTOP` es favorito; `PEER_TTL=15 s`).
- [ ] **DECIDIR (antes de tocar Android):** núcleo headless vs foreground-only (`android/SPEC.md`)

## 🟠 Seguridad (fuera de fase, chico)
- [ ] **Autostart sin comillas (CWE-428)**: la entrada de autostart (`HKCU\...\Run`) que escribe `tauri-plugin-autostart` no lleva comillas → *unquoted path* con rutas con espacios. Hoy funciona por la heurística de Windows, pero conviene reescribirla con comillas. (Estaba anotado dentro de la línea de Fase 3; NO se tocó en esa fase — el plugin controla el quoting, hay que post-procesar la entrada del registro.)

## 🟡 Cuando se pueda
- [ ] **Sin autenticación mutua del cliente (no mTLS)** — el server HTTPS usa `with_no_client_auth`, así que NO verifica la identidad de quien envía. Cualquier gate por `sender_fingerprint` (el de `/clipboard`, y el de `/text` si algún día se cierra) es spoofeable por quien conozca una huella conocida (viaja en la TXT de mDNS/QR). El cert pinning de Fase 3 (Tarea 3.1) protege al EMISOR (pin del receptor), no al receptor contra un emisor falso. Cerrarlo = client-cert pinning bidireccional (mTLS): cambio de handshake en ambos lados + compat con peers viejos. Grande; no urgente (la app es solo-LAN). Anotado desde Fase 3.
- [ ] **Zombie-killer mata una instancia SANA en doble-launch** (pre-existente, NO regresión de Fase 2; confirmado por el review). El binario ya se llama `millennium-clipboard.exe`, así que el killer siempre mató a la instancia viva al relanzar, defeateando el "enfocar ventana" de single-instance. Hoy tolerable (el estado está persistido y se recarga). Fix correcto: chequear liveness (probe HTTPS `/info`) antes de matar — solo matar al que NO responde (el zombie real). Es más grande; no urgente.
- [ ] **Fragilidad del harness de test en Windows** (parcialmente resuelto en la Fase 2). Agregar tests al crate rompe la carga del binario de test del lib (comctl32-v6 sin manifest → `STATUS_ENTRYPOINT_NOT_FOUND`). La salida fue extraer la lógica testeable a `src-tauri/displays-tests/`, un crate sin Tauri ni `windows` que **sí** corre en CI (ADR-011). **Lo que sigue sin correr**: los 4 tests de `displays/mod.rs` (mock, orden, centinela 0x0, precisión de u64), que dependen de tipos que viven en `mod.rs` y ese archivo arrastra el resto del módulo. Para cerrarlo: mudar `DisplayView` + `mark_can_detach` + `sort_for_display` a un archivo propio windows-free y sumarlo a `displays-tests`. Chico y mecánico.
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
