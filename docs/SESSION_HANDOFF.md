# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-23 · **Branch**: `feat/displays-v2` (mergeada a `main` por FF) · **Working tree**: limpio.

## En una línea

**Displays v2 COMPLETA (Fase 1 "perfiles con superpoderes" + Fase 2 "rediseño: displays como sección con
pestañas CLIP|DISP + banner global") — IMPLEMENTADA, verificada en hardware, y RELEASEADA como `v1.3.0`
final.** `main` quedó al día por fast-forward y los dos specs se archivaron. Próximo trabajo: dos
candidatos (elegir en chat nuevo) — **el fix de la caché del updater** (recomendado, chico) o la **Fase 3
(audio por perfil)** con el Arquitecto.

## Lo que se hizo esta sesión

- **Fase 2 implementada** (rediseño estructural, puro frontend — backend intacto): pestañas de nivel
  superior **CLIP | DISP** en el HUD (absorbieron el botón DISP; gateadas por userAgent → en Android no
  aparecen); displays pasó de `#displays-modal`/`.modal-backdrop` a `<section>` a pantalla completa
  (`.displays-shell`, hija de `.app`, comparte la fila 1fr con el clipboard); el cambio de sección OCULTA
  con `[hidden]` (no desmonta) → un transfer del clipboard sobrevive; el reloj del auto-revert salió a un
  **banner GLOBAL** (`#app-banners`, ámbar) visible desde cualquier sección; ESC/CLOSE vuelven a Clipboard.
- **Verificado E2E** (Playwright + mock `window.__TAURI__` sobre datos SMOKE, sirviendo `src/` estático):
  8/8 criterios del spec. Gates: `node --check` + `cargo check` verdes.
- **2 regresiones cazadas y corregidas**: botón CONF fuera de pantalla a 1080 (→ etiquetas CLIP/DISP +
  media query que esconde HOST/NODE <1040px); CLIP/DISP apretados en `is-mobile` (hallazgo del review
  adversarial → `html.is-mobile .hud-sections:not([hidden]){display:contents}`).
- **Review adversarial multi-agente** (4 dimensiones, cada hallazgo verificado): 3 limpias, 1 hallazgo bajo
  (el grid mobile) corregido.
- **Verificación en HARDWARE**: ★ primario / atajos / sobreescribir perfiles OK; la UI nueva (pestañas
  CLIP|DISP, sección a pantalla completa) renderiza bien; transferencias entre PCs OK.
- **Release final `v1.3.0`**: bump en los 3 archivos del guard, FF de `main`, tag `v1.3.0` (sin sufijo →
  `release.yml` lo publica como FINAL, la landing lo sirve). Ambos specs (Fase 1 + Fase 2) archivados en
  `docs/archive/` con "✅ IMPLEMENTADO".

## En qué estado quedó

- **`main` = `feat/displays-v2` = `v1.3.0`** (FF, pusheado). Tag `v1.3.0` pusheado → `release.yml`
  compilando. **Verificar que salió verde** en Actions o con la API pública:
  `curl -s https://api.github.com/repos/guidocameraeq/Millennium-Clipboard/releases/tags/v1.3.0` (200 =
  publicado; `prerelease` debe ser `false`).
- **Hardware**: Fase 1 y la UI de la Fase 2 confirmadas. **Único sin probar en hardware**: criterio #1
  (que un transfer EN CURSO sobreviva el salto CLIP↔DISP) — **salteado por decisión de Guido, bajo riesgo**
  (verificado a nivel código por identidad de nodo del DOM; el motor de transferencia no se tocó).

## Próximo paso CONCRETO (al retomar) — elegir en chat NUEVO

- 🅰️ **Fix de la caché del updater (recomendado primero, chico).** Tras un update, el WebView2 sirve el
  frontend VIEJO cacheado hasta borrar `%LOCALAPPDATA%\com.guidocameraeq.millennium\EBWebView` → esta
  sesión perdió rato porque Guido veía la versión nueva con la UI vieja. Afecta CADA update en CADA PC.
  Fix de fondo (backend): que la app limpie su caché al detectar cambio de versión al arrancar, o servir
  los assets con `Cache-Control: no-cache`. Su mini-spec (delta) + una beta para probar. Detalle en
  `docs/TODO.md` (🟠 Auto-update...).
- 🅱️ **Fase 3 — audio por perfil (grande).** Al aplicar un perfil, cambiar el output de audio por default
  de Windows. Net-new, requiere INVESTIGACIÓN (API tipo `IPolicyConfig`) + extender qué guarda el perfil
  (dato del usuario → migración). La diseña **el Arquitecto**, arrancando por un spike. Backlog en
  `docs/TODO.md` → 🟣 Displays v2.

## Bloqueos

- **El CI de `v1.3.0` tiene que salir verde** (`release.yml` ~12 min; publica como release FINAL). Si sale
  rojo, ver el log en Actions.

## Contexto que no está en otros docs

- **Bug de caché del updater** (ver próximo paso 🅰️): descubierto en vivo. Los datos del usuario (perfiles,
  favoritos) viven en `...\Roaming\com.guidocameraeq.millennium\` (NO se tocan al limpiar la caché); la
  caché del WebView2 está en `...\Local\...\EBWebView`. Workaround manual: cerrar del todo (bandeja →
  Salir; la X manda a bandeja y NO cierra) → borrar `EBWebView` → reabrir.
- **Cómo se verifica E2E el frontend** (el proyecto no tenía harness): copiar `src/` a un dir servible,
  inyectar un `mock-tauri.js` (define `window.__TAURI__` con `invoke`/`listen` falsos + datos SMOKE + un
  `__emit` para disparar eventos del backend + un wrap de `setInterval` para contar timers activos), servir
  estático y manejar con Playwright. El frontend NO carga sin el mock (`main.js` usa `window.__TAURI__`).
  Sirve para toda la Fase 3 que sea frontend.
- **`gh` está en `C:\Program Files\GitHub CLI\gh.exe`** (el bash no lo tiene en PATH — usar ruta completa o
  PowerShell). El repo es **PÚBLICO**: el estado de un release se ve con la API pública sin auth.
- **El fingerprint-mismatch entre 2 PCs** que apareció al probar transferencias era un pin viejo
  pre-existente (la identidad `identity.json` de la laptop es de mayo, intacta) — se resolvió solo al
  actualizar. NO es un tema de la Fase 2. Si reaparece: en la PC que da el error, FORGET al peer y dejar
  que reaparezca en ALL (re-pinea la huella actual).
- **`docs/SPEC-displays.md`** (roadmap general de displays) sigue vivo: su 🔵 en el TODO tiene sub-checks
  físicos de Fase 3 (auto-revert desde AJUSTES, watcher `WM_DISPLAYCHANGE`) que faltan.
