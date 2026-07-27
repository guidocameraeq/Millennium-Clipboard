# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-27 · **Branch**: `feat/displays-v2` (= `main` por FF) · **Working tree**: limpio tras el commit de este cierre · **Último commit**: `docs: cierre de sesión 2026-07-27 …`

## En una línea

**Sesión de DISEÑO con el Arquitecto: nació el spec de Displays v2 Fase 4 "perfiles como escenas" — está READY, pendiente CONSTRUIR.** No se tocó código; el entregable es `docs/SPEC-displays-v2-fase4.md`. Al aplicar un perfil, además de mover monitores (F1/2) y cambiar audio (F3), va a **disparar acciones** (abrir Steam Big Picture / un juego / Chrome con cuenta+link, fijar volumen) y **cerrar lo que abrió al volver**. Próximo trabajo (chat nuevo): **construir la Fase 4** (spec listo) o el **fix de la caché del updater** (🟠, chico).

## Lo que se hizo esta sesión

- **Lluvia de ideas + investigación + spec delta** para la Fase 4, con el Arquitecto (Modo B). **No se escribió código de la app** — sesión de diseño.
- **Investigación real** (subagentes con fuentes, no de memoria): cómo lanzar Steam Big Picture y apps/URIs en Windows, HDMI-CEC, control de TV por red, prior-art de escenas/macros (Stream Deck, Home Assistant, DisplayFusion), y Chrome perfil+URL. Titular: lo jugoso (abrir Steam) es lo **más fácil**; el control de TV es lo frágil.
- **Decisiones tomadas con Guido**:
  - Escenas: **2-3 fijas** (jugar / ver / trabajar); cualquier perfil puede ser escena (sin tipo aparte).
  - Al volver: la escena **cierra sola** lo que abrió (modelo ida-y-vuelta), **atado al COMMIT** del cambio (si el video se auto-revierte, no corre ninguna acción).
  - Piezas: **una sola acción `Lanzar`** (destino + args) cubre Big Picture, juego, Chrome cuenta+link y el **"comando libre"**; + **volumen por perfil**.
  - **Control de TV y HDMI-CEC: DESCARTADO** — la TV de Guido es una **TCL con Google TV** (caso frágil ADB/Android TV, no el fácil Roku); la maneja con el control.
  - "Ver pelis" = Chrome con una **cuenta específica** (`--profile-directory` = nombre de carpeta) + link.
- **Spec escrito y aprobado**: `docs/SPEC-displays-v2-fase4.md` (READY), calcado de la Fase 3 — AGREGA / MODIFICA / **NO SE TOCA**, tabla "esto cambio / esto preservo", y la **trampa #1 de `update_settings`** heredada (debe copiar `profile_actions` tal cual o se borran en silencio).

## En qué estado quedó

- **No se tocó código de la app** → nada que compilar (`cargo check`/`node --check` N/A esta sesión). El único cambio es el spec nuevo.
- `main` sigue en **`v1.4.0`** (Displays v2 Fase 1+2+3 COMPLETO), sin cambios funcionales esta sesión.

## Próximo paso CONCRETO (al retomar) — elegir en chat NUEVO

- 🅰️ **Construir la Fase 4** (spec listo). Abrir chat nuevo en la carpeta y pegar: `inicio — ejecutá el spec docs/SPEC-displays-v2-fase4.md (está READY)`. Verificar con `/smoke` (criterio #1: lo de NO SE TOCA sigue andando). **Dos asteriscos a verificar E2E en hardware**: (a) cerrar Big Picture por `steam://close/bigpicture` en Windows (no confirmado; plan B `Alt+Enter` o manual); (b) que el orden (foto de monitores → acción) haga caer Steam en la TV.
- 🅱️ **Fix de la caché del updater** (🟠, chico): tras un update, el WebView2 sirve el frontend viejo cacheado hasta borrar `EBWebView`. Afecta cada update en cada PC. Detalle en `docs/TODO.md` (🟠 Auto-update).

## Bloqueos

- Ninguno.

## Contexto que no está en otros docs

- **El spec ya trae toda la data de la investigación** (comandos de Steam con registro/`-start`, sintaxis de Chrome `--profile-directory` + fullscreen best-effort, por qué CEC y control de TV quedaron afuera). No hace falta re-investigar al construir.
- **La TV de Guido es una TCL con Google TV** → si algún día se retoma "control de TV por red", es el camino **frágil** (ADB / Android TV Remote v2), no el fácil (Roku). Quedó FUERA del alcance de la Fase 4 a propósito.
- **Molde a reusar al construir** (calcado de Fase 3): el estado paralelo `audio_previo` en `mod.rs` es el molde para `escena_activa`; el enganche del audio en `cargar_perfil` (2 ramas) + `aplicar_perfil_directo` es donde van las acciones; la "trampa #1" de `update_settings` (vendor `monarch`) aplica igual al nuevo `profile_actions`.
