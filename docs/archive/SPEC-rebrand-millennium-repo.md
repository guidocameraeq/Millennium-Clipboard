# SPEC: Presentación del repo "Millennium" (combo) — README + landing + About
Rebrandear la presentación PÚBLICA del repo Millennium para que represente el combo (Clipboard + Displays), no solo el clipboard. Puro repo/contenido/branding: NO toca la app ni saca release.
- Estado: ✅ IMPLEMENTADO 2026-07-29
- Fecha: 2026-07-29

## Por qué (el dolor)
El repo `Millennium` es el combo (Clipboard + Displays, release final v1.5.1), pero su README y su landing se escribieron ANTES de Displays: son 100% clipboard, no mencionan la palabra "Displays", y todavía se presentan como "Millennium Clipboard". Un visitante no se entera de que existe la mitad Displays — que es un producto grande y ya releaseado. Hay que poner la presentación a la altura del combo.

## Contexto (de la exploración — real)
- **Displays (releaseado, final en v1.5.1)**: attach/detach de TV con **auto-revert** (si algo sale mal, vuelve solo); **perfiles-escena** (monitores + audio + apps de un click); **audio por perfil** (cambia los 3 roles de Windows); primario + lienzo de arrastre; **perfil de arranque + atajos globales**; watcher en vivo (~0% CPU). Motor **CCD** (de Monarch): valida antes de aplicar + verifica re-enumerando + escalera de rescate/rollback → **nunca te deja sin pantalla**. *(Honestidad: "cerrar apps al salir de la escena" quedó como plan-B en el spec; se publicita "abre tu setup solo", NO "lo cierra solo".)*
- **README** (rama `main`, solo-EN, ~39 líneas): título `◣◢ Millennium Clipboard`; secciones *What it is / Features (6, todas clipboard) / Download / Stack / License: TBD*. Los links ya apuntan a `Millennium` (arreglados en la partición).
- **Landing** (rama `gh-pages`, `index.html` autocontenido ~2.7 MB): estética synthwave/HUD, cyan `#00f0ff` + magenta `#ff2bd6`, fuentes Orbitron/Share Tech Mono embebidas base64, **bilingüe EN/ES** (dict `I18N` + atributos `data-i18n` + `localStorage "mc-lang"` + detección por `navigator.language`, voseo rioplatense). Estructura: nav (wordmark `MILLENNIUM` + badge `v1.0.0` + toggle EN/ES) → hero ("Your clipboard, across the room") → `#features` (6 cards) → `#showcase` (3 shots) → `#how` (3 pasos) → `#download` → footer ("MILLENNIUM CLIPBOARD"). Todo clipboard; versión mostrada `v1.0.0` (vieja).
- **App**: ya tiene el violeta de Displays `--app-accent #b45cff` y el switch CLIPBOARD/DISPLAYS. **NO se toca** en esta tanda.
- **About del repo**: description `"cortapapeles del conurbano"` (chiste, solo-clipboard), homepage OK (`github.io/Millennium`), **sin topics**.

## AGREGA (lo nuevo)
- **Contenido de Displays** para README y landing: pitch + lista de features, en **EN y ES**, agrupado (perfiles-escena · attach/detach + auto-revert · audio por perfil · arranque/atajos · watcher).
- **Capturas de Displays** para el showcase: generadas con el **mock del frontend** (`window.__TAURI__` mockeado + Playwright, sirviendo `src/` con datos de ejemplo), embebidas base64 en la landing (patrón autocontenido). Guido puede reemplazarlas por reales después.
- En la **landing**: hero reencuadrado al combo (dualidad cyan/violeta), una **sección Displays** con su grilla de features + su showcase, y el acento **violeta `#b45cff`** para la parte Displays. Copy nuevo en los DOS idiomas del dict `I18N`.
- En el **README**: sección Displays (features + para qué sirve), y el combo reflejado en "What it is" y "Stack".
- **About**: topics del combo + descripción (default propuesto abajo; Guido decide).

## MODIFICA (lo existente — con su efecto colateral)
- **README** (`main`): título → paraguas "Millennium" (dos apps); "What it is" describe el combo; "Features" se parte en **Clipboard + Displays**; "Stack" suma Displays/CCD. *Efecto: es un doc, no toca la app.*
- **Landing** (`gh-pages` `index.html`): reestructura hero/features/showcase/how/download/footer/meta para el combo; badge de versión `v1.0.0` → **`v1.5.1`**; link de descarga → release **v1.5.1**; `<title>`/OG → combo. *Efecto: mantener INTACTO el mecanismo i18n (todo copy nuevo con su par EN/ES o se rompe la paridad) y la estética (self-contained, CSP-safe, `prefers-reduced-motion`).*
- **About del repo**: description + topics.

## NO SE TOCA (el seguro)
- **La app**: nada de `src-tauri/` ni de `src/` (el frontend) se modifica en esta tanda → **no hay release, no hay riesgo** para la app diaria. *(El mock para las capturas NO cambia el frontend; solo lo sirve con datos falsos y le saca fotos.)*
- **El wordmark "CLIPBOARD //" del logo** (`src/index.html:34`): queda para la tanda cosmética futura (decidido con Guido), junto con Android.
- **La estética y el mecanismo bilingüe de la landing**: se EXTIENDEN, no se reemplazan. El voseo rioplatense del ES se mantiene.
- **Los releases, el binario `millennium-clipboard.exe`, el identifier y el updater**: intactos.
- **La mitad Clipboard** del README/landing (el contenido que ya está bueno): se conserva; se le SUMA Displays, no se reescribe de cero.

## Criterios de aceptación (verificables)
- **(Regresión)** La app diaria y sus releases NO se tocan; la landing sigue self-contained (sin dependencias externas, CSP-safe) y respeta `prefers-reduced-motion`.
- El README DEBE presentar las DOS apps (Clipboard + Displays) con sus features, bajo la marca "Millennium".
- La landing DEBE mostrar Displays (hero + grilla de features + al menos 1 showcase con captura) además de Clipboard, con el acento violeta en la parte Displays.
- Todo copy nuevo de la landing DEBE existir en EN y ES (paridad i18n intacta) — verificable cambiando el toggle EN/ES.
- La landing DEBE mostrar la versión **v1.5.1** y su link de descarga DEBE apuntar al `.exe` de v1.5.1.
- Ningún link muerto; `<title>`/OG describen el combo.
- El About DEBE tener descripción del combo + topics.

## Supuestos
- [MEDIO] Las capturas de Displays generadas con el mock alcanzan para el showcase; Guido las reemplaza por reales si quiere.
- [BAJO] License sigue "TBD" salvo que Guido elija una.
- [BAJO] Descripción propuesta del About (Guido puede mantener su chiste o mezclar): *"Two Windows tools in one app — LAN clipboard & file sharing + a display/monitor manager. No cloud, no accounts."*

## Riesgos y decisiones ⚠️
- ⚠️ **Solo-repo, sin release** (decidido): esta tanda no toca la app. Consecuencia: el wordmark del logo sigue diciendo "CLIPBOARD" hasta la tanda cosmética — inconsistencia menor y sabida.
- ⚠️ **Paridad bilingüe**: si se agrega copy sin su par ES, se rompe el toggle. Consecuencia: todo string nuevo va en AMBOS objetos del dict `I18N`. Es el criterio #1 de verificación de la landing.
- ⚠️ **Honestidad del copy**: no prometer "cierra las apps solo" (plan-B en el spec de Displays); sí "abre tu setup solo". Consecuencia: copy fiel a lo que hace de verdad.
