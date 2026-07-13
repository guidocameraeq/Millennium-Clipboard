# Contexto compartido — Millennium Clipboard

> **Leé este archivo ANTES de ejecutar cualquier fase.** Cada spec de fase asume todo lo que está acá y no lo repite.

Este documento es el "manual de a bordo" para un agente (Opus 4.8) que va a ejecutar el plan de remediación. Contiene: qué es la app, cómo está armado el código, cómo compilar/correr/verificar, las convenciones a respetar, y las reglas de trabajo.

---

## 1. Qué es la app

Millennium Clipboard es una utilidad **solo-LAN** (sin nube, sin cuentas) hecha con **Tauri 2** que comparte **texto, archivos y portapapeles** entre dispositivos de la misma red Wi-Fi. Targets: **Windows desktop** (`.exe` portable) y **Android** (`.apk` sideload). Versión actual: **0.15.0**.

- **Backend:** Rust (~5.5k LOC en `src-tauri/src/`).
- **Frontend:** JavaScript vanilla + CSS, sin framework ni bundler (~5k LOC en `src/`). Un único IIFE con un objeto `state` global; estado **por push** (escucha `listen('peers-changed')`), comandos vía `invoke()`.
- **Estética:** TRON / outrun neón ("GRID"). El README menciona Win98/typewriter pero el código real es la grilla neón.
- **Descubrimiento:** hoy corren **tres mecanismos a la vez** — mDNS (`_millennium._tcp.local`), broadcast UDP (`:53318`), y un poller de sondeo TCP `/info` cada 6 s.
- **Transporte:** HTTPS (axum + rustls) con certificados auto-firmados por dispositivo; el "fingerprint" es el SHA-256 del certificado DER.
- **Puertos:** servidor HTTPS `53319` (con fallback `53320..53328`); UDP `53318`.

El dueño usa la versión de escritorio a diario pese a los bugs; la de Android **nunca funcionó bien**. El diagnóstico completo que originó este plan está resumido en la sección 8.

---

## 2. Mapa del código

### Rust — `src-tauri/src/`
| Archivo | Responsabilidad |
|---|---|
| `lib.rs` (1534) | Orquestación. ~29 comandos Tauri, `run()`/setup, spawn de los pollers, tray, panic hook, `apply_update`, clipboard poller. **El corazón.** |
| `discovery.rs` (721) | mDNS (register + browse) + el "presence poller" TCP + el `PeerMap` compartido. |
| `udp_discovery.rs` (292) | Broadcast UDP del "hello" cada 5 s + recepción. |
| `manual_peers.rs` (92) | Store JSON de peers agregados a mano. |
| `http_server.rs` (998) | Servidor axum: `/info`, `/text`, `/prepare-upload`, `/upload`, `/progress`, `/clipboard`, `/clipboard/image`. Consent oneshot, `safe_join`. |
| `http_client.rs` (448) | Cliente reqwest **pooled** (OnceLock), subida por streaming, resume por Range. |
| `identity.rs` (113) | Certificado auto-firmado + fingerprint + `compute_local_ip()`. |
| `thumbnails.rs` (77) | Generación de miniaturas de imágenes. |
| `clipboard_sync.rs` (112) | Store de sync + hashing + supresión de eco (loop prevention). |
| `settings.rs`, `preferences.rs`, `aliases.rs`, `icon_overrides.rs` | Stores JSON (favoritos, ajustes, alias, iconos). **Copy-paste entre sí.** |
| `runtime_log.rs` (153) | Ring buffer de log (cap 5000) + **emite un evento IPC por línea** + escribe a archivo. |
| `updater.rs` (230) | Auto-update vía GitHub Releases + `.bat` que swapea el `.exe`. |
| `windows_integration.rs` (247) | AUMID (toasts), icono de ventana, "mata-zombis" de procesos. |
| `android_fs_bridge.rs` (128) | Puente SAF/MediaStore (Android) para `content://` y `/Downloads`. |
| `main.rs` (6) | Entry point que llama a `millennium_clipboard_lib::run()`. |

### Android — `src-tauri/gen/android/app/src/main/`
| Archivo | Responsabilidad |
|---|---|
| `java/com/guidocameraeq/millennium/MainActivity.kt` | Activity Tauri; arranca el runtime Rust y el servicio. |
| `java/com/guidocameraeq/millennium/MillenniumService.kt` | Foreground service: MulticastLock + notificación persistente. |
| `AndroidManifest.xml` | Permisos, tipo de FGS, componentes. |
| `res/xml/network_security_config.xml` | Config de tráfico (cleartext/TLS). |
| `res/xml/file_paths.xml` | FileProvider paths. |
| `../build.gradle.kts` | `compileSdk=36`, `minSdk=24`, `targetSdk=36`, firma release. |

> El proyecto Android en `gen/android/` es **regenerable** con `tauri android init`, PERO estos pocos archivos se commitean a propósito (ver `.gitignore` líneas 71-79). **Editá estos archivos directamente; no corras `tauri android init`** (borraría los cambios).

### Frontend — `src/`
| Archivo | Responsabilidad |
|---|---|
| `main.js` (2012) | Toda la lógica. IIFE con `state` global, `listen()` para eventos, `invoke()` para comandos. |
| `index.html` (545) | Markup estático. |
| `styles.css` (2587) | TRON/outrun + **tres sistemas de CSS móvil peleando**. |

### Config
| Archivo | Nota |
|---|---|
| `src-tauri/Cargo.toml` | **No tiene `[profile.release]`** (Fase 0 lo agrega). |
| `src-tauri/tauri.conf.json` | `bundle.active=false`, `security.csp=null`, `withGlobalTauri=true`. |
| `.cargo/config.toml` | `+crt-static` en Windows (exe portable sin VC++ redist). |
| `package.json` | Solo el script `tauri`; devDep `@tauri-apps/cli`. |
| `.keystore/millennium-release.jks` | Keystore de firma Android (versionado). |

---

## 3. Cómo compilar y correr

Todos los comandos se corren desde la raíz del proyecto: `D:\Millenium Clipboard\millennium-clipboard`. Shell principal: **PowerShell**; Bash disponible para scripts POSIX.

### Desktop (Windows)
```powershell
# Desarrollo con hot-reload del frontend (no minifica; sirve ../src crudo):
npm run tauri dev

# Build release. Como bundle.active=false, NO genera instalador:
npm run tauri build
# → el artefacto es:  src-tauri\target\release\millennium-clipboard.exe
#   (para releases se renombra a "Millennium Clipboard.exe" a mano — ver updater.rs:19-22)

# Chequeo/compilación rápida solo del backend:
cd src-tauri; cargo check ; cargo clippy
```
`+crt-static` hace que el `.exe` sea portable (no requiere VC++ Redistributable).

### Android
Requisitos: Android SDK + NDK, `JAVA_HOME`, y **`src-tauri/gen/android/keystore.properties`** (gitignored) con:
```properties
keyAlias=<alias>
keyPassword=<pass>
storeFile=<ruta absoluta a .keystore/millennium-release.jks>
storePassword=<pass>
```
```powershell
# Dev en dispositivo/emulador conectado:
npm run tauri android dev

# Build release firmado (.apk):
npm run tauri android build --apk
# → src-tauri\gen\android\app\build\outputs\apk\universal\release\
```
> **No corras `tauri android init`** (regenera y pisa `MainActivity.kt`, `MillenniumService.kt`, el manifest, etc.).

El código Android en Rust está detrás de `#[cfg(target_os = "android")]`. Para verlo compilar de verdad usá `tauri android build` (no `cargo check` normal, que compila para el host).

---

## 4. Convenciones a respetar

- **Logging:** usá `runtime_log::info/warn/error(...)` — NO `println!`. Rutea al buffer en memoria (cap 5000), al archivo, y (hoy) a un evento IPC por línea. La Fase 0 cambia la parte del IPC; respetá esa API.
- **Comandos Tauri:** `#[tauri::command] async fn ...` registrados en el `invoke_handler![...]` de `lib.rs`. El estado se accede con `tauri::State<AppState>` (montado con `app.manage(AppState { ... })`).
- **Locking:** todo `std::sync::Mutex` se toma en un scope corto y se **clona antes de cualquier `.await`**. **Nunca sostener un lock a través de un await.** Mantené esta disciplina (es de lo mejor del código).
- **Async:** trabajo bloqueante (IO de archivos, decodificación de imágenes, arboard) va en `tokio::task::spawn_blocking`. No bloquees el reactor.
- **Stores JSON:** patrón `load() -> unwrap_or_default()` + `persist() -> fs::write`. La Fase 2 lo reemplaza por escritura atómica; después de esa fase, usá el store genérico nuevo.
- **Frontend:** un solo `state` global; render de peers por **diff incremental** (`buildPeerItem`/`updatePeerItem`), no `innerHTML` masivo. Eventos delegados. Cada `invoke()` va con `try/catch` + revert optimista. **Escapá** cualquier string que venga de un peer antes de meterlo al DOM (`escapeHtml`, o mejor `textContent`/`createElement`).
- **Compatibilidad de protocolo:** hay peers viejos en la red. **No rompas el formato del hello UDP ni el JSON de `/info`** sin necesidad; si extendés, hacelo con campos opcionales.

---

## 5. Flujo de trabajo (git y verificación)

- El repo git está en `millennium-clipboard/` (el directorio interno).
- **Un commit por Tarea** (o por fase si son chicas). Mensaje claro en imperativo. Ejemplo: `fase0: clipboard poller solo codifica en cambio real + gate por peers`.
- **No** hagas `git push` ni cambies de branch salvo que el humano lo pida. Trabajá sobre la rama actual (o creá `remediation/<fase>` si el humano lo prefiere).
- **Antes de decir "listo":** compilá (`cargo check` / `npm run tauri build`), y verificá el criterio de "Cómo verificar" de la fase. **Evidencia antes que afirmaciones.** Si algo no se pudo verificar, decilo.
- No hay suite de tests hoy. Donde una Tarea lo pida, **agregá un test unitario Rust** (`#[cfg(test)] mod tests`) y corré `cargo test`. Para lo visual/perf, verificá corriendo la app y observando (Task Manager, panel de LOG, etc.).

---

## 6. Orden de ejecución recomendado

**Windows** (cada fase deja la app mejor y es shippable sola):
1. `windows/phase-0-stop-the-bleed.md` — el consumo de CPU/RAM. **Empezá acá.**
2. `windows/phase-1-discovery.md` — el parpadeo de peers (Rust compartido).
3. `windows/phase-2-correctness.md` — pérdida de datos y bugs de UI.
4. `windows/phase-3-security.md` — pinning real de certificado, CSP.

**Android** (asume el core arreglado; ver `android/SPEC.md` para la decisión estratégica previa):
- A → `android/phase-A-lifecycle-and-approval.md` (lo de mayor impacto)
- B → `android/phase-B-discovery-and-storage.md`
- C → `android/phase-C-clipboard-qr-mobile.md`

Dependencias entre specs (están anotadas en cada archivo): la remoción del filtro `/24` y la reconciliación de IP viven en **Windows Fase 1** y las comparte Android Fase B. El **pinning de certificado** (Windows Fase 3) es prerrequisito para confiar en transferencias de Android.

---

## 7. Ejecutar una fase (para el agente Opus)

Trabajá **una fase por vez**, en orden. Para cada fase:
1. Leé el archivo de la fase completo.
2. Leé los archivos de código que cita, confirmá que el estado actual coincide (los números de línea pueden haber cambiado).
3. Implementá las Tareas en orden.
4. Compilá y corré "Cómo verificar".
5. Commiteá.
6. Reportá qué hiciste, qué verificaste y qué quedó abierto.

Si una instrucción del spec choca con la realidad del código, **priorizá arreglar el problema raíz descrito** sobre seguir el texto al pie de la letra, y anotá la divergencia.

---

## 8. Resumen del diagnóstico (por qué existe este plan)

Auditoría multi-agente (2026-07-06), hallazgos verificados contra el código real:

- **Consumo de CPU/RAM en PC (verificado):** (1) el clipboard poller re-codifica PNG+base64 la imagen entera cada 500 ms antes del dedup y del chequeo de peers; (2) cada línea de log se emite por IPC y el frontend la concatena a un string infinito (O(n²)); (3) la capa visual (grilla animada + `backdrop-filter` + `mix-blend-mode`) nunca deja de repintar, sin `prefers-reduced-motion`.
- **Parpadeo de peers:** 3 mecanismos de descubrimiento escriben un mismo mapa sin prioridad; el UDP conoce la IP correcta pero **la descarta a propósito**; un filtro `/24` purga peers alcanzables.
- **Android nunca funcionó:** la aprobación vive en un WebView congelado en segundo plano; el FGS `dataSync` se corta a las ~6 h sin `onTimeout`; `START_STICKY` revive un shell sin runtime Rust; recibir un archivo lo carga entero en RAM; la sync de portapapeles es un stub.
- **Seguridad:** el "fingerprint pinning" no valida el certificado TLS (`danger_accept_invalid_certs(true)` + fingerprint auto-reportado en JSON); CSP nula + `withGlobalTauri` + strings de peers en `innerHTML` = XSS con acceso a IPC.
- **Núcleo a conservar:** motor de transferencia (cliente pooled, streaming, resume), identidad (cert + fingerprint), disciplina de locks, frontend push-based con diff incremental. **No reescribir la app.**

El informe navegable completo se generó como artifact aparte.
