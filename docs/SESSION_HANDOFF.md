# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-21 · **Branch**: `feat/displays-v2` · **Working tree**: limpio (tras el commit de cierre).

## En una línea

**Displays v2 Fase 1 ("perfiles con superpoderes") IMPLEMENTADA, verificada LOCAL (gates verdes + review
adversarial) y pre-releaseada como `v1.3.0-beta.1` — pero NO verificada en hardware todavía.** Además,
la **Fase 2 (rediseño) quedó con spec READY**: Guido eligió la Opción A (pestañas arriba) mirando
mockups, y el Arquitecto escribió + red-teó `docs/SPEC-displays-v2-fase2.md`. Próximo paso: **probar la
beta en hardware**, y después ejecutar la Fase 2 en chat nuevo.

## Lo que se hizo esta sesión

- **Fase 1 de Displays v2 completa** (4 features, todas apoyadas en lo que el motor de Monarch ya
  soportaba, sin tocar el vendor):
  - **★ primario** por monitor en LISTA (apply en vivo, pasa por la red de auto-rollback).
  - **Perfil de arranque**: al bootear con `--autostart` aplica ese perfil **directo, sin red**; no-op si ya coincide.
  - **Atajos globales por perfil** (`tauri-plugin-global-shortcut` v2.3.2, gateado a Windows) + interruptor general; disparan el perfil directo. Conflicto con otro programa → avisa y no registra.
  - **Botón "↻ actualizar"** por perfil (pisa con el layout actual, con confirmación).
- **Verificación local** (todo verde): crate scratch reusado (2 ramas cfg, sin warnings), vendor (22
  tests), displays-tests (13), `node --check`, `cargo metadata` (resolvió el plugin + fijó el lock).
- **Review adversarial multi-agente** (4 dimensiones + verificación de cada hallazgo): **4 bugs reales
  corregidos** — el freeze del subsistema si `confirm` rebota tras un apply directo, la clase `capturing`
  que no se limpiaba, la carrera de `saveSettings` que podía borrar el perfil de arranque, y el label
  ON/OFF desincronizado.
- **Pre-release `v1.3.0-beta.1`**: rama `feat/displays-v2` con 2 commits (feature `e54dbe4` + bump
  `fcb408d`), versión subida en los 3 archivos del guard, guard verificado LOCAL, tag pusheado →
  `release.yml` compilando en GitHub.
- **Mockups Fase 2**: artifact HTML con 3 opciones de navegación (pestañas / lado a lado / barra
  lateral). **Guido eligió la A (pestañas arriba).**
- **Spec Fase 2 READY**: `docs/SPEC-displays-v2-fase2.md` (rediseño, Opción A), con el **red-team
  incorporado** (8 huecos tapados: ESC/CLOSE sin destino, el reloj atado a `!displaysModal.hidden` en 4
  lugares, la sub-pestaña, el transfer en curso, la vara de regresión).

## En qué estado quedó

- **`feat/displays-v2`** = Fase 1 (código + bump a 1.3.0-beta.1), pusheada. **`main` sigue en 1.2.0** (la
  Fase 1 NO está mergeada; se FF al sacar el release final, como se hizo con la Fase 3).
- **`v1.3.0-beta.1`**: el tag está pusheado; `release.yml` estaba compilando al cierre. **Verificar que
  salió verde** en GitHub (Actions) — desde acá no se puede (no hay `gh`, repo privado).
- **Hardware**: NADA de la Fase 1 se probó en el monitor real todavía. Es lo primero al retomar.

## Próximo paso CONCRETO (al retomar)

1. **Verificar la beta `1.3.0-beta.1` en hardware**: cuando el CI esté verde, instalarla por el
   auto-updater (Settings → APP UPDATES → CHECK) y probar en el desktop real: ★ primario, el atajo
   aplicando un perfil, el startup profile (dejar la TV, reiniciar con autostart), + regresión
   (clipboard/transferencias) + **CPU en reposo ~0% en el Task Manager**.
2. Si la Fase 1 anda: sacar el **release final** (tag `v1.3.0` **sin** sufijo → la landing lo sirve),
   **FF `main`** a `feat/displays-v2`, y **archivar** `docs/SPEC-displays-v2.md` (Fase 1) a
   `docs/archive/` con "✅ IMPLEMENTADO". Si aparece un bug, arreglarlo antes del final.
3. **Ejecutar la Fase 2** (rediseño) → chat NUEVO → `inicio — ejecutá el spec
   docs/SPEC-displays-v2-fase2.md (está READY)`. **Decisión de secuencia** (está en el spec): conviene
   verificar la Fase 1 en hardware ANTES de arrancar la 2 (si la beta tiene un bug latente, la 2 lo
   heredaría).

## Bloqueos

Ninguno. (El CI del release corre en GitHub; verificar verde.)

## Contexto que no está en otros docs

- **El crate scratch de verificación local se reutilizó** (de la sesión previa, en el temp): apunta por
  `#[path]` al `displays/mod.rs` real y tiene el build de `windows 0.60` cacheado → `cargo check` en las
  2 ramas en ~1-2s. Al reconstruirlo, la receta sigue en DECISIONS (sin `winreg`, con `anyhow`).
- **`lib.rs` NO se compila local** (dlltool) → los comandos, el plugin y el handler del hotkey **solo los
  valida el CI**. Se cuidó siguiendo los patrones existentes + la API del plugin verificada contra
  fuentes (Rust-only NO necesita tocar `capabilities/`).
- **La Fase 1 (`SPEC-displays-v2.md`) NO se archiva todavía**: espera la verificación en hardware (su
  propia regla "cuando esté verificada" en la línea final del spec).
- **Mockups Fase 2** (artifact publicado, Opción A elegida) y el **groundwork** viven en el scratchpad de
  la sesión (efímeros); lo durable quedó en el spec de la Fase 2 + este handoff.
- **El CI corre ante cualquier push** a la rama (incluidos los de solo-docs) — sigue como TODO
  (`paths-ignore`).
