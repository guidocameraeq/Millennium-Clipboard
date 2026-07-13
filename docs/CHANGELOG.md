# CHANGELOG — Millennium Clipboard

> Historia permanente. `/cierre` agrega una entrada AL TOPE en cada sesión. Orden descendente estricto, sin excepciones. Nada de versiones duplicadas en otros docs.

## 2026-07-13 — montaje del sistema de trabajo

### Added
- Sistema de trabajo del playbook: `CLAUDE.md`, skills `/inicio` `/cierre` `/smoke`, hooks (SessionStart + check-code), `.claude/settings.json`.
- Documentación operativa: `docs/SESSION_HANDOFF.md`, `docs/TODO.md` (sembrado con las fases del spec de remediación), `docs/CHANGELOG.md`.

### Changed
- `.gitignore`: se ignora `.claude/settings.local.json` (el resto de `.claude/` — skills, hooks, settings — se versiona).
