# SPEC: "Dos apps en una" — marco Clipboard ⇄ Displays — Millennium Clipboard
Rediseña el esqueleto de la app: un switch grande arriba separa Clipboard y Displays como dos apps, cada una con su color y su barra; los ajustes se parten en tres cajones. Solo frontend (HTML/CSS/JS de chrome), cero backend.
- Estado: **IMPLEMENTADA 2026-07-28** (frontend completo, verificado con Playwright + review adversario; prerelease `v1.5.0-beta.2` para que Guido lo pruebe por el updater). Archivada. El release FINAL + merge a `main` quedan pendientes de que la beta ande bien.
- Fecha: 2026-07-28

## Por qué (el dolor)
Hoy Clipboard y Displays conviven mal: se alternan con dos botoncitos chicos arriba a la derecha (`CLIP|DISP`), y la barra superior mezcla botones "de nadie" (SCAN·QR·LOG·CONF) que en realidad son de Clipboard. Los ajustes son un solo panelón que revuelve config del programa (updater, arranque) con config de Clipboard (descargas, notificaciones). No se lee como "dos herramientas en un programa", que es lo que es. Cuando esto funcione, vas a saber de un vistazo en qué app estás (por el color), cada app va a tener lo suyo a mano, y los ajustes van a estar donde corresponden.

## Contexto del código (de la exploración — nombres y líneas REALES)
- `switchSection(section)` (`main.js:3078`) YA alterna `'clipboard'`/`'displays'`: oculta/muestra `#clipboard-section` (`.grid`) y `#displays-section`, togglea `.is-active` en los `.hud-section-btn`, y corre `enterDisplaysSection()`/`leaveDisplaysSection()`. **Es el motor del cambio y se reusa.** Hace early-return si ya estás en esa sección (`main.js:3081`); en el boot `state.section` ya vale `'clipboard'` (`main.js:209`) sin llamar a `switchSection`.
- Las pestañas de sección viven en `#hud-sections` (`main.js:1616`); `hudSectionBtns` se arma con `hudSections.querySelectorAll('.hud-section-btn')` (`main.js:1617`); el activo se togglea por `b.dataset.section` (`main.js:3087`); el click se despacha por `data-action==='section-clipboard/displays'` (`main.js:1098-1101`). El reveal es `if (hudSections && displaysEnabled) hudSections.hidden = false` (`main.js:1657`), con `displaysEnabled = !android` (`main.js:1656`).
- Los `.hud-btn` se atan **por-elemento al cargar**: `document.querySelectorAll('.hud-btn').forEach(btn => btn.addEventListener('click', ...))` (`main.js:1079`). NO es delegación en un padre: mover un botón en el DOM es inocuo, pero **sacarle/renombrarle la clase `.hud-btn` lo mata**. Los `data-action`: `refresh` (SCAN), `history` (LOG), `qr` (QR), `settings` (CONF).
- El botón `#displays-close` ("← CLIPBOARD") hace `switchSection('clipboard')` (`main.js:3138`).
- Ajustes: un solo `#settings-modal` abierto por CONF (`openSettingsModal()`, `main.js:1235`). Plomería atada al **elemento modal**, no a los controles: foco de apertura `focusFirstControl(settingsModal)` (`main.js:1275`), botón cerrar (`main.js:1282`), Escape global `if (!settingsModal.hidden) closeSettingsModal()` con fallback `if (!modalClosed && state.section==='displays') switchSection('clipboard')` (`main.js:3278-3284`), click-afuera `settingsModal.addEventListener('click', ...)` (`main.js:3439`). Grupos: GENERAL (descargas `#settings-download-dir`, efectos `#settings-fx`), TRANSFERS **`.desktop-only`** (`index.html:509`; auto-aceptar `#settings-auto-accept`, notificaciones `#settings-notifications`), SYSTEM **`.desktop-only`** (`index.html:534`; arranque `#settings-autostart`, bandeja `#settings-close-tray`), UPDATES.
- Displays YA tiene sus ajustes: la sub-pestaña AJUSTES (`data-pane="settings"`) con auto-revert (`#displays-revert-secs`), perfil de arranque (`#displays-startup-profile`), atajos (`#displays-shortcuts-enabled`). **No se toca.**
- Colores: `styles.css` NO tiene `--app-accent` ni `[data-app]` (0 usos). Tiene **222 usos de `var(--neon-cyan)`** hardcodeados por toda la hoja. `--neon-magenta` (`styles.css:52`) YA se usa en Clipboard: el label FIRST CONTACT del modal de archivos entrantes (`main.js:1152`).
- Banners de nivel app en `#app-banners` (`index.html:659-687`): auto-revert **ámbar** (`#displays-pending`) y backend-error **rojo**, ambos con botones `.modal-btn`.
- Render push-based / diff incremental: `renderPeers`/`buildPeerItem`, `renderDisplays`/`buildDisplayItem`, `renderProfiles`/`buildProfileItem`, `renderCanvas`, `setText`/`textContent`.

## AGREGA (lo nuevo)
- **Switch grande** arriba-centro (dos segmentos CLIPBOARD / DISPLAYS, con profundidad estilo "Vidrio Profundo": el activo flota y prende en su color). Cada segmento lleva su color fijo aunque esté apagado.
- **Barra de herramientas por app** bajo el switch: en Clipboard, HOST/NODE + SCAN·QR·LOG·AJUSTES; en Displays, el contador de monitores + REFRESH.
- **⚙ APP** (botón neutro, ni cyan ni violeta, en una zona neutra del top bar) → panel **Ajustes de la app**: updater + arranque con Windows + cerrar a bandeja + efectos visuales.
- **Panel Ajustes de Clipboard** (cyan): carpeta de descargas + auto-aceptar de favoritos + notificaciones de escritorio.
- Lenguaje de profundidad (opción C) en el chrome nuevo y adentro de Displays: dato hundido, controles que flotan.

### Mecanismo del color por app (decidido — para que la sesión fresca no improvise)
Como hoy NO hay token semántico (222 cyan hardcodeados), el color por app se hace así, **acotado**:
1. Se define en `:root`: `--app-accent`, `--app-accent-glow`, `--app-accent-soft`, `--app-accent-dim`, **con el valor de los `--neon-cyan*` de hoy** (o sea: por defecto la app está en cyan; el boot ya está teñido sin tocar nada).
2. Se **redefinen solo esos tokens a violeta** (`#b45cff` + derivados) cuando la app activa es Displays, con un selector de raíz: `body[data-app="displays"] { --app-accent: … }`.
3. Usan `--app-accent` (no cyan directo): **(a)** el chrome nuevo (switch, barras por app, borde del marco `.app`, franja de estado); **(b)** las reglas **propias de Displays** — se convierten los `var(--neon-cyan*)` de las reglas `.displays-*`/`.display-*` (bloque `styles.css` ~2900-3565) a `--app-accent`, más overrides **scopeados a `#displays-section`** de los `.modal-*` que Displays reusa.
4. **Quedan en `--neon-cyan` (no se tocan)**: la sección Clipboard entera, los demás modales (archivos entrantes, peer, QR, agregar-peer, settings), y los banners de `#app-banners`.
- **El magenta** (`#ff2bd6`) marca "lo elegido" **dentro** de Displays (pantalla principal, perfil cargado); donde ya se usa hoy (FIRST CONTACT) queda igual.

## MODIFICA (lo existente que se toca, con su efecto colateral a cuidar)
- **`index.html` — barra superior**: el contenedor de las pestañas de sección se convierte en el switch grande **conservando el id `hud-sections`** (o se actualizan las 3 referencias `main.js:1616,1617,1657`). Cada segmento lleva **las tres cosas**: clase `.hud-btn hud-section-btn`, `data-action="section-clipboard|section-displays"` **y** `data-section=…`. **Efecto a cuidar**: si le falta `.hud-btn` no recibe el click (`main.js:1079`); si le falta `data-action` no dispara; si le falta `data-section`/`.hud-section-btn` no se resalta el activo (`main.js:1617,3087`).
- **Mover SCAN/QR/LOG + settings** de `.hud-right` a la barra de Clipboard: **conservan su `.hud-btn` y su `data-action`**. **Efecto a cuidar**: el handler es por-elemento sobre `.hud-btn` (`main.js:1079`) → moverlos es seguro, **renombrar/quitar `.hud-btn` los mata en silencio**.
- **`switchSection()`**: se le suma marcar el color activo (`document.body.dataset.app = section`). Como el boot no llama a `switchSection` (early-return, `main.js:3081`), el color inicial lo garantiza el **default cyan de `:root`** del mecanismo de arriba (no hace falta setear nada al arrancar). **Efecto a cuidar**: no alterar la lógica de mostrar/ocultar ni el gateo de Android.
- **Se elimina `#displays-close`**: el switch cumple esa función (su binding `switchSection('clipboard')` se cubre con el switch). **Efecto a cuidar**: nada más debe depender de ese botón.
- **Partir `#settings-modal` en dos** (panel App disparado por ⚙ APP, panel Clipboard disparado por AJUSTES). **Efecto a cuidar (lo que el red-team encontró)**: cada panel nuevo necesita **su propia plomería a nivel-modal**, no alcanza con conservar ids de controles:
  - Entrada propia en la cadena de Escape (`main.js:3278`), y el **fallback a Clipboard (`main.js:3284`) debe correr solo si NINGÚN panel quedó abierto** (si no, apretar Escape en Displays con un panel abierto te saca de sección con el panel colgado).
  - Su `addEventListener('click')` de backdrop (espejo de `main.js:3439`).
  - Su `focusFirstControl` al abrir (`main.js:1275`) y su botón/handler de cerrar (`main.js:1282`).
  - **Todos los controles conservan su `id`** (los handlers están atados por id) **y su `.desktop-only`** donde la tienen (SYSTEM `index.html:534`, TRANSFERS `index.html:509`), para que en Android sigan ocultos. La persistencia (los `invoke` al backend) no se toca.
- **`styles.css`**: reglas nuevas para switch, tematización por app (ver Mecanismo), paneles nuevos y profundidad C; conversión acotada de los cyan del bloque Displays a `--app-accent`. **Efecto a cuidar**: la tematización se scopea (`#displays-section` / chrome nuevo) para **no alcanzar** los otros modales ni `#app-banners`; cuidar especificidades para no pisar estilos ajenos.

## NO SE TOCA (el seguro de no romper)
- **Todo el backend Rust** (`src-tauri/`): intacto. Cero comandos nuevos, cero cambios. Es solo frontend de chrome.
- **El motor de transferencias** (cliente pooled + streaming + resume) y la **identidad** (cert + fingerprint): intactos.
- **El patrón push-based / diff-incremental**: `renderPeers`/`buildPeerItem`, `renderDisplays`/`buildDisplayItem`, `renderProfiles`/`buildProfileItem`, `renderCanvas`, `updateProfileItem`, `setText`/`textContent` — no se tocan.
- **El escapado**: todo lo del backend entra al DOM con `textContent`/`createElement`, nunca `innerHTML` con datos. El chrome nuevo es 100% estático.
- **Compatibilidad de protocolo**: hello UDP y JSON de `/info` — sin cambios (esto ni se acerca a la red).
- **Displays por dentro** (Fase 1-4): auto-revert y su banner global, perfiles-como-escenas, atajos, lienzo con arrastre, la sub-pestaña AJUSTES. Su lógica no cambia; solo hereda color violeta y profundidad por CSS.
- **Los otros modales** (archivos entrantes, peer, QR, agregar-peer, y los propios `.modal-*` compartidos fuera de `#displays-section`): siguen en cyan, no los alcanza la tematización.
- **Los banners de `#app-banners`** (auto-revert ámbar, backend-error rojo): mantienen su color; la tematización se scopea para no teñirlos.
- **El magenta donde ya vive** (label FIRST CONTACT, `main.js:1152`): intacto.
- **Los 222 usos de `--neon-cyan` de la hoja en general** (fuera del bloque Displays y del chrome nuevo): NO se reescriben. Retintar TODA la app sería rediseño total y queda FUERA.
- **El store de ajustes y de perfiles** (JSON): mismas claves, mismos valores, mismo backend. Los ajustes se **re-ubican** en la UI, NO se migran ni se renombran.
- **El gateo de Android**: `displaysEnabled` sigue mandando; en Android la app queda solo-Clipboard.

## FUERA de alcance (qué NO entra)
- Retintar la sección Clipboard o los modales generales (siguen cyan).
- Cambiar el tono/paleta más allá de sumar el violeta de Displays (calibración fina del `#b45cff` es visual, no arquitectura).
- Tocar la lógica interna de cualquier feature (transferencias, displays, updater): solo se re-ubica y re-pinta el chrome.
- Android: no se agrega el switch ni Displays (queda como hoy, solo-Clipboard).

## Criterios de aceptación (verificables)
1. **Regresión**: todo lo de NO SE TOCA funciona igual que antes — transferir/recibir texto y archivos, favoritos, y todo Displays por dentro (auto-revert, perfiles/escenas, atajos, lienzo) sin cambios.
2. Al **abrir la app** (sin tocar nada) el marco ya está en cyan; CUANDO tocás el switch a DISPLAYS, el marco pasa a violeta (y vuelve a cyan al volver), y el botón "← CLIPBOARD" ya no existe.
3. En Clipboard, SCAN·QR·LOG·AJUSTES viven en la barra de Clipboard y hacen exactamente lo mismo que hoy (rescan, QR, log, ajustes) — ninguno queda mudo.
4. El **⚙ APP** abre updater + arranque con Windows + bandeja + efectos; **AJUSTES de Clipboard** abre descargas + auto-aceptar + notificaciones; Displays mantiene su pestaña AJUSTES. CUANDO cambiás cualquier ajuste, se guarda igual que hoy (mismo backend, mismas claves).
5. Con **cualquiera de los dos paneles nuevos abierto**, Escape lo cierra y el click-afuera también, **sin cambiar de sección** (estando en Displays, Escape no te patea a Clipboard).
6. En **Android**: no aparece el switch (ni el segmento DISPLAYS) — la app queda solo-Clipboard como hoy; los ajustes siguen alcanzables y los controles `.desktop-only` (arranque, bandeja, transferencias) siguen ocultos.
7. CUANDO entrás a Displays, toma toda la pantalla con sus pestañas LISTA/PERFILES/AJUSTES/LIENZO, y el banner de auto-revert sigue visible desde cualquier app **con su color ámbar** (no teñido).
8. El switch y los controles nuevos tienen **foco de teclado visible** (`:focus-visible`) y respetan `prefers-reduced-motion`. Todo texto sobre violeta (`#b45cff`) o cyan cumple contraste **AA ≥ 4.5:1**; si el violeta no llega como texto chico, se usa un violeta más claro para texto y el `#b45cff` queda para bordes/glows.
9. SI Displays devuelve error o no hay monitores, ENTONCES lo muestra igual que hoy (el rediseño no cambia esos estados).

## Supuestos
- [BAJO] Los efectos visuales (grid/scanline) van en "App general" porque pintan toda la pantalla. Guido puede moverlos a Clipboard si prefiere.
- [BAJO] El WebView2/Edge objetivo soporta las features CSS usadas (`:has()`, custom properties anidadas) — el proyecto ya las usa.
- [BAJO] El violeta `#b45cff` es el tono base; ajustable en revisión visual sin cambiar la arquitectura (salvo que no pase AA como texto → se aclara).
- [BAJO] En Android, el panel ⚙ APP muestra solo lo que aplica (updater + efectos); arranque/bandeja siguen `.desktop-only` y ocultos.

## Riesgos y decisiones ⚠️
- ⚠️ **Color por app (cyan/violeta)** con el mecanismo `--app-accent` acotado (chrome + bloque Displays; el resto queda cyan). Consecuencia: es un cambio de identidad visible y es la decisión más cara de revertir. Si más adelante quisieras **toda** la app violeta, eso toca los 222 usos de `--neon-cyan` de la hoja = rediseño total, otro proyecto. **Decidido con Guido** (eligió "un color por app" y el violeta de la opción C).
- ⚠️ **Reestructurar la barra superior** (switch héroe + barras por app): toca la estructura de `index.html`. Consecuencia: el riesgo real es perder la clase `.hud-btn` o un `data-action` al restilar → el botón muere en silencio. Por eso el criterio #3 es regresión dura.
- ⚠️ **Partir el settings modal en dos**: Consecuencia: sin la plomería a nivel-modal (Escape/backdrop/foco/cerrar por panel), Escape y click-afuera quedan rotos y aparece la regresión "Escape en Displays te saca de sección". Mitigación: criterio #5.
- ⚠️ **Datos guardados (ajustes/perfiles)**: NO se migran ni renombran — mismas claves, mismo backend. Solo cambia dónde se ven. (Sin tabla cambio/preservo: no cambia ningún dato del usuario.)
- ⚠️ **Sacar "← CLIPBOARD"**: Consecuencia: el camino de vuelta ahora es el switch (más descubrible, pero cambio de hábito).
- ⚠️ **Backend / protocolo**: NO se tocan. Consecuencia de mantenerlo así: cero riesgo de romper la red o los peers viejos; todo el riesgo queda contenido en el frontend de chrome.
