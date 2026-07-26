# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-25 · **Branch**: `feat/displays-v2` (= `main` por FF) · **Working tree**: limpio salvo lo anotado · **Último commit**: `docs: cierre …`

## En una línea

**Displays v2 Fase 3 "audio por perfil" — IMPLEMENTADA, verificada en hardware por Guido, y RELEASEADA como `v1.4.0` final.** Al aplicar un perfil, el sonido de Windows se manda a la salida que el perfil tenga asignada (los 3 roles). `main` al día por FF, spec archivado. **Con esto Displays v2 (Fase 1+2+3) queda COMPLETO.** Próximo trabajo (elegir en chat nuevo): **el fix de la caché del updater** (🟠, recomendado, chico) o la **Fase 4 "perfiles como escenas"** (con el Arquitecto).

## Lo que se hizo esta sesión

- **Fase 3 implementada** siguiendo el spec `SPEC-displays-v2-fase3.md` (ahora archivado). Backend Rust + frontend, en 6 capas:
  - **Dato**: `AudioTarget {endpoint_id, friendly_name}` + `AppSettings.profile_audio: BTreeMap<String,AudioTarget>` (side-map por NOMBRE de perfil, mismo patrón que `profile_shortcuts`, `#[serde(default)]` → compat con stores viejos). En el **vendor** `monarch`.
  - **COM** (`src-tauri/src/displays/audio.rs`, nuevo): `IMMDeviceEnumerator` para enumerar salidas activas y leer el default; **`IPolicyConfig` declarado a mano** (interfaz COM no documentada, con cascada de IIDs Win7+ → Vista) para setear el default. Molde RAII COM calcado de `DesktopWallpaperSession` (apply.rs). Best-effort, sin panics (`panic=abort`).
  - **Enganche** en `mod.rs`: audio se aplica en `cargar_perfil` (las DOS ramas, incluida la no-op = "botón único" solo-audio), en `aplicar_perfil_directo` (arranque + atajos), con estado paralelo `audio_previo` en `Interno` para el rollback; se restaura en `revert()` y en el auto-revert del watchdog (`reportar_desenlace`), se descarta en `confirm()`.
  - **Comandos Tauri** `displays_list_audio_outputs` / `displays_set_profile_audio` / `displays_clear_profile_audio` + registro.
  - **Frontend**: dropdown "Sonido a:" por perfil (`buildProfileItem`/`updateProfileItem`, por diff, sin pisar la selección abierta) + toast del evento `displays-audio` cuando la salida no está.
- **Trampa #1 blindada**: `update_settings` (vendor) copia `profile_audio` TAL CUAL del input (nunca `Default`), con **test** que lo prueba.
- **Verificado**: `cargo check` del scratch crate **verde y sin warnings en AMBAS ramas** (Windows + linux/Android); **25/25 tests del vendor** (3 nuevos: trampa #1, compat de store viejo, round-trip); **review adversarial** de 6 dimensiones (3 hallazgos LOW: 2 arreglados —guardar el previo en fallo parcial + cascada de IIDs—, 1 documentado como decisión segura); `node --check` OK.
- **Verificado en HARDWARE por Guido**: el sonido efectivamente va a la salida del perfil (criterio #3) y vuelve al auto-revertirse (criterio #5) — el gate que el spec ponía para cerrar.
- **Release**: `v1.4.0-beta.1` (prerelease por el updater para probar) → confirmado → **`v1.4.0` final** (tag sin sufijo → la landing lo sirve). FF de `main`, spec archivado.

## En qué estado quedó

- **`main` = `feat/displays-v2` = `v1.4.0`** (FF, pusheado). Tag `v1.4.0` pusheado → `release.yml` publica el release FINAL.
- **Verificar que el CI de `v1.4.0` salió verde** (Actions o API pública):
  `curl -s https://api.github.com/repos/guidocameraeq/Millennium-Clipboard/releases/tags/v1.4.0` → `prerelease` debe ser `false` y el `.exe` en `assets`.
- El build local sigue roto por toolchain (`dlltool`) — es lo conocido; el `.exe` sale del CI. El código se chequea con el scratch crate (ver DECISIONS.md).

## Próximo paso CONCRETO (al retomar) — elegir en chat NUEVO

- 🅰️ **Fix de la caché del updater (recomendado, chico).** Tras un update, el WebView2 sirve el frontend VIEJO cacheado hasta borrar `%LOCALAPPDATA%\com.guidocameraeq.millennium\EBWebView`. Afecta CADA update en CADA PC. Fix de fondo: que la app limpie su caché al detectar cambio de versión al arrancar, o `Cache-Control: no-cache`. Mini-spec (delta) + una beta. Detalle en `docs/TODO.md` (🟠 Auto-update).
- 🅱️ **Fase 4 — "perfiles como escenas": acción al aplicar (con el Arquitecto).** Que un perfil lance una app/comando (ej. "jugar en la tele" → TV primaria + audio a la TV + abrir Steam Big Picture). Más fácil que el audio (lanzar proceso/URL). Ideas hermanas: wallpaper por perfil, volumen por perfil. Backlog en `docs/TODO.md` (🟣).

## Bloqueos

- Ninguno. (Verificar el verde del CI de `v1.4.0` es lo único pendiente de este cierre — si sale rojo, ver el log en Actions.)

## Contexto que no está en otros docs

- **Cómo se verificó el COM de audio sin poder buildear local**: se agregó al scratch crate (DECISIONS.md) la feature **`Win32_System_Variant`** (además de las 3 que nombraba el spec) — es la que destraba `PROPVARIANT` + `IPropertyStore::GetValue` + `PropVariantToStringAlloc` en windows-rs 0.60. `PropVariantToStringAlloc` vive en `System::Com::StructuredStorage`, NO en `UI::Shell::PropertiesSystem`. El `IPolicyConfig` a mano (vtable manual, `SetDefaultEndpoint` es el método #11) **compila y anda** en 0.60.
- **`lib.rs` NO pasa por el scratch** (depende de tauri, no compila local) → su único gate es el CI. Auditarlo a mano antes de taggear: esta vez tenía un `Ok(...)` sin rama de error (tipo ambiguo) que se anotó explícito (`Ok::<_, String>(...)`).
- **Bug de caché del updater** (ver próximo paso 🅰️): datos del usuario en `...\Roaming\...` (intactos al limpiar caché); caché del WebView2 en `...\Local\...\EBWebView`. Workaround: cerrar del todo (bandeja → Salir) → borrar `EBWebView` → reabrir.
- **`docs/SPEC-displays.md`** (roadmap general de displays) sigue vivo: su 🔵 en el TODO tiene sub-checks físicos (auto-revert desde AJUSTES, watcher `WM_DISPLAYCHANGE`) que faltan.
