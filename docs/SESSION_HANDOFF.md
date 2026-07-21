# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-21 · **Branch**: `main` · **Working tree**: limpio (tras el commit de cierre).

## En una línea

**La Fase 3 del SPEC-displays está HECHA, verificada en hardware (núcleo) y RELEASEADA como v1.2.0
(release final).** Todo el trabajo de displays (Fases 1–3) quedó en `main` por fast-forward. La próxima
misión ya está definida y con backlog escrito: **"Displays v2"** — las features que Guido esperaba y no
estaban + algunas nuevas (ver `docs/TODO.md`, sección 🟣). **Esa es una misión NUEVA: chat nuevo +
Arquitecto.**

## Lo que se hizo esta sesión

- **Fase 3 completa** (perfiles, ajustes, watcher `WM_DISPLAYCHANGE`, lienzo de arrastre opción A):
  ver la entrada del CHANGELOG del 2026-07-21. Gate local verde + 2 rondas de review adversarial (5
  hallazgos, los 5 corregidos).
- **Prerelease `v1.2.0-beta.1`** (primera corrida real de `release.yml`) → Guido lo instaló por el
  **auto-updater** desde 1.1.0 y probó en hardware: perfiles, lienzo y **auto-revert** OK.
- **Release final `v1.2.0`**: bump de versión, fast-forward de `main` (5ffdfca..fbaedb4), tag `v1.2.0`
  pusheado → `release.yml` publica el final (la landing empieza a servir 1.2.0).
- **Backlog de "Displays v2" capturado** en el TODO con triage (ver Próximo paso).

## En qué estado quedó

- **main** = todo displays (Fases 0–3). `feat/displays` == `main` (mismo commit).
- **v1.2.0**: el tag está pusheado; el `release.yml` del final estaba corriendo al cierre. **Verificar
  que salió verde** y que la landing sirve 1.2.0 (si no aparece, el CI se puso rojo).
- **Hardware**: núcleo verificado (perfiles, lienzo, auto-revert, updater). **Sub-checks menores
  pendientes** (en TODO, sin urgencia): plazo desde AJUSTES, watcher en vivo al enchufar/desenchufar, y
  la regresión (transferencia/clipboard + CPU en reposo en el Task Manager).

## Próximo paso CONCRETO (al retomar)

**Ejecutar la Fase 1 de "Displays v2" → chat NUEVO → `inicio — ejecutá el spec
docs/SPEC-displays-v2.md (está READY)`.** El Arquitecto ya diseñó y blindó la Fase 1 (2026-07-21): el
spec está **READY** en `docs/SPEC-displays-v2.md` — "Perfiles con superpoderes" (primario elegible,
aplicar-perfil-al-iniciar, atajos de teclado, botón actualizar). Decisiones tomadas con Guido: los
aplicados sin mirar (startup/atajos) van **directo, sin la red**; el detach manual y el lienzo la
conservan. Las fases 2 (rediseño), 3 (audio por perfil) y "resolución por perfil" siguen en el backlog
(`docs/TODO.md` → 🟣 Displays v2) y llevan su propio spec cuando toque.

**Insight que ahorra trabajo**: 3 de los "faltantes" (elegir primario, shortcuts de perfil, aplicar-al-
iniciar) **el motor de Monarch YA los soporta** (`OutputConfig.primary`, `AppSettings.profile_shortcuts`,
`startup_profile_name`) — la Fase 3 no los cableó a la UI, nada más. Baratos. Lo caro y net-new: el
**cambio de audio por perfil** (investigar la API de Windows, no está en Monarch) y el **rediseño**
(displays full-screen, dos secciones grandes, deja de ser pop-up).

De paso, en el próximo uso, cerrar los 3 sub-checks físicos menores → con eso el SPEC-displays queda
COMPLETO y se archiva a `docs/archive/`.

## Bloqueos

Ninguno.

## Contexto que no está en otros docs

- **Verificación local de displays**: crate scratch (receta en DECISIONS, Nota de verificación) — al
  reconstruirlo, sacar `winreg` (arrastra `dlltool`) y agregar `anyhow`. Las 2 ramas de `cfg`
  (Windows + linux-gnu) son el gate local; `displays-tests` + `vendor/monarch` corren `cargo test`.
- **El overwrite de perfiles YA funciona** (guardar con el mismo nombre → banner → reemplaza); Guido no
  lo descubrió. En Displays v2 sumar un botón "actualizar perfil" más obvio.
- **El CI corre en cada push, incluidos los de solo-docs** (tarea abierta: `paths-ignore` en build.yml).
