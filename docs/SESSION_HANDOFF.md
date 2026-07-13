# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-13 · **Último commit**: f6ca4a2 — v0.15.0 (el montaje del sistema y `docs/` están SIN commitear)

## Qué se hizo
- Se montó el **sistema de trabajo** del playbook sobre el proyecto (que ya existía y anda): `CLAUDE.md`, skills `/inicio` `/cierre` `/smoke`, hooks (SessionStart + check-code) y `settings.json`.
- Se armó la **documentación operativa**: este HANDOFF, `TODO.md` (sembrado con las fases del fix), `CHANGELOG.md`.
- El **spec del fix ya existía** en `docs/remediation/` (del audit 2026-07-06) — no se tocó.

## Estado
- Branch: la actual (rama única). Working tree: `docs/` (spec + operativos) y `.claude/` **nuevos, sin commitear**; `CLAUDE.md` nuevo sin commitear.
- Build: no se compiló nada esta sesión (fue solo montaje). App en v0.15.0.

## En curso
- Nada. Sistema montado y listo para ejecutar el fix.

## Próximo paso CONCRETO
Ejecutar la **Fase 0 de Windows**: leer `docs/remediation/00-SHARED-CONTEXT.md` + `docs/remediation/windows/SPEC.md`, después implementar `docs/remediation/windows/phase-0-stop-the-bleed.md` en orden, compilar, correr `/smoke`, y verificar el consumo en el Task Manager antes de seguir a la Fase 1.

## Bloqueos
- **Android** tiene una decisión estratégica previa que el humano debe tomar: **núcleo headless vs foreground-only** (`docs/remediation/android/SPEC.md`). No arrancar Android sin decidirla.

## Archivos tocados
- `CLAUDE.md`, `.claude/skills/{inicio,cierre,smoke}/SKILL.md`, `.claude/hooks/{session-start.sh,check-code.cjs}`, `.claude/settings.json`, `docs/{SESSION_HANDOFF,TODO,CHANGELOG}.md`, `.gitignore`.

## Contexto que no está en otro doc
- **Abrí los chats de trabajo en la carpeta INTERNA** `D:\Millenium Clipboard\millennium-clipboard` (no en la de afuera). Si se abre en la de afuera, el hook y las skills no se cargan.
- El **autostart** del PC apunta a un `.exe` 7 versiones viejo en una ruta de Desktop que ya no existe → arranque silenciosamente roto. Lo arregla la Fase 0.
- No hay suite de tests hoy; cada fase que lo pida agrega tests unitarios Rust.
