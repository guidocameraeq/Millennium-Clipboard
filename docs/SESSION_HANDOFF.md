# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-29 · **Repo**: renombrado a `guidocameraeq/Millennium` (era `Millennium-Clipboard`; la carpeta local sigue con el nombre viejo) · **Branch**: `main` · **Working tree**: limpio tras este cierre · **Último release**: `v1.5.2` (final) · **Último commit**: el de este cierre.

## En una línea

**Se partió el proyecto en DOS productos y se pulió cada uno.** El repo pasó a llamarse **`Millennium`** = el COMBO (Clipboard + Displays, la app diaria de Guido), que debutó como release final **`v1.5.1`** (finalizó el shell "dos apps en una" + Displays Fase 4 que estaban en beta, + rebrand visible a "Millennium") y recibió una cosmética **`v1.5.2`** (updater directo + logo "GRID"). Se creó un repo NUEVO **`MillenniumClipboard`** con el clipboard limpio (v1.0.0, sin Displays). Los dos repos quedaron con su README, landing (bilingüe el del combo), About y su propio updater. **Nada de la app diaria se rompió** — se actualizó por el updater de siempre en cada paso.

## Lo que se hizo esta sesión

**1. Partición del repo en dos** (spec `SPEC-particion-repos.md`, archivado; ADR-003 en el hub):
- **Renombrado** `Millennium-Clipboard` → `Millennium` en GitHub. La app instalada sigue por el **redirect 301 permanente**.
- **Camino elegido** (validado con red-team adversario): nombre DISTINTO para el clipboard (`MillenniumClipboard`, sin guion) → el redirect del nombre viejo vive para siempre, **sin puerta irreversible**. (Reusar el nombre / mover el combo → descartados por el riesgo de orfanar la app diaria.)
- **`MillenniumClipboard` creado** (API con la credencial de git; no hay `gh`): sembrado desde `5ffdfca` (v1.0.0, **87 commits**, clipboard puro sin displays), updater apuntándose a sí mismo, workflows del CI portados, landing propia (gh-pages), release **v1.0.0** final. Corregido el default branch (había quedado en `gh-pages` → `main`). Notas del release + About (descripción + topics).

**2. Debut de Millennium (el combo) — release final `v1.5.1`**:
- Merge `feat/shell-dos-apps` → `main` (FF; entraron el shell + Displays Fase 4 + 2 bumps de CI Node 24 de origin/main).
- **Rebrand visible a "Millennium"** (`a8bb4b7`): productName + título de ventana ("Millennium // GRID") + tooltip del tray + DisplayName de notificaciones + título del release. **NO** se tocó el binario `millennium-clipboard.exe` ni el identifier.

**3. Rebrand del repo Millennium (presentación)** (spec `SPEC-rebrand-millennium-repo.md`, archivado):
- **README** al combo (sección Clipboard + sección Displays).
- **Landing** (`gh-pages`) rebrandeada: hero reencuadrado, **sección Displays en violeta** (`#b45cff`) con 6 features + showcase con **captura real** (generada con mock de `__TAURI__` + Playwright), **bilingüe EN/ES** (paridad 67=67), v1.5.1, links a Millennium/latest. Verificada en vivo con Playwright.
- **About**: descripción del combo + 10 topics.

**4. Cosmética `v1.5.2`** (spec `SPEC-cosmetica-millennium-v152.md`, archivado):
- `updater.rs:15` const → `guidocameraeq/Millennium` (directo; auto-curativo, sin depender del redirect).
- `index.html:34` logo-sub `CLIPBOARD //` → `GRID //` (el logo lee "MILLENNIUM · GRID").

## En qué estado quedó

- **`Millennium` (combo)**: `main`, release final **v1.5.2** (`.exe` + digest, CI verde, verificado por API). Updater apunta directo a `Millennium`. Landing combo bilingüe viva. README/About combo.
- **`MillenniumClipboard` (clipboard)**: `main`, release **v1.0.0** final, 87 commits, sin displays (404), landing viva, updater a sí mismo, README/About clipboard.
- **Hub** (`../`): ADR-003 commiteado **local** (el hub no tiene remote).
- **Compilación**: el `.exe` sale del **CI** (build local roto por dlltool). CI de v1.5.1 y v1.5.2 **verde**.

## Lo que quedó en curso / próximo paso CONCRETO

1. **[Esperando a Guido] Instalar `v1.5.2` por el updater** (Settings → APP UPDATES → CHECK). Le llega por el updater normal (vía redirect), **sin descarga manual**. Al instalarlo: el logo dice **"MILLENNIUM · GRID"** y el updater queda directo a Millennium. ⚠️ Si ve el logo VIEJO tras actualizar → caché del WebView2 (TODO 🟠): cerrar la app → borrar `%LOCALAPPDATA%\com.guidocameraeq.millennium\EBWebView` → reabrir.
2. **[Diferido] Rename de Android** a "Millennium" (`app_name` / notificación del servicio / nombre del `.apk`): se hace cuando se compile un APK nuevo (otra tubería, NO viaja en el `.exe`). Es lo único del rebrand que queda.
3. **[Opcional, no-release]** Reemplazar la captura del showcase de la landing por una real del setup de Guido · revertir el chiste "cortapapeles del conurbano" en el About del combo si lo quiere.

## Bloqueos
- Ninguno. Todo verificado por CI + API. La única E2E pendiente es que Guido instale v1.5.2 (acción suya).

## Contexto que no está en otros docs
- **El repo local es el mismo de siempre**: era `Millennium-Clipboard`, ahora el `origin` es `Millennium.git`. La CARPETA local sigue llamándose `Millennium-Clipboard/` (cosmético, no se tocó). Se hizo `git fetch --unshallow` (era un clon shallow que impedía pushear la historia completa a MillenniumClipboard).
- **Crear repos / setear metadata en GitHub sin `gh`**: se usó la credencial guardada de git (`git credential fill` → token → API con `Authorization: Bearer`), **sin exponer el token**. Sirve para `POST /user/repos`, `PATCH` del repo (description/homepage/**default_branch**), `PUT` topics, `POST` pages.
- **Gotcha del CI en repo nuevo**: un tag pusheado juntito con `main` a un repo recién creado NO dispara el workflow (Actions aún no lo registró) → la cura es **re-pushear el tag**. Pasó con MillenniumClipboard.
- **GitHub pone el default branch al PRIMER branch que llega**: como el push de `main` falló al principio (shallow) y `gh-pages` entró primero, MillenniumClipboard quedó con default `gh-pages` (el repo parecía VACÍO). Se corrigió por API a `main`.
- **La landing es bilingüe con un dict `I18N` (en/es) + `data-i18n`**: todo copy nuevo va en AMBOS idiomas o se rompe la paridad. El violeta de Displays se logró con un **override scopeado de `--cyan` en `#displays`** (el CSS usa `var(--cyan)`).
