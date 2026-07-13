---
name: cierre
description: Ritual de cierre de sesión de Millennium Clipboard. Usar cuando el usuario dice "cierre", "cerrá la sesión", "voy a comprimir", "compact ya", "prepará todo para compactar" o "cierre parcial".
---

# /cierre — cerrar la sesión (después el chat se descarta)

Dos modos:

- **`cierre`** (default, cierre completo): docs al día → commit → push → reporte. Después el usuario descarta el chat y la próxima misión arranca con `/inicio` en un chat nuevo.
- **`cierre parcial`** (la emergencia): el contexto se llenó a mitad de misión. Ejecutar SOLO los pasos 1-2 + avisar "listo para `/compact`". Sin push obligatorio; el chat sigue. Tras el `/compact`, el hook SessionStart re-inyecta el handoff — asumir que NO leí ningún archivo.

## Pasos (modo completo)

1. **Verificar consistencia**: releer `docs/SESSION_HANDOFF.md` (el previo) + `git status` + `git log` de la sesión. Si se corrió la app o se tocó un dispositivo real (transferencia, Android), verificar su estado real y comparar contra los docs — **la realidad gana**.
2. **`docs/SESSION_HANDOFF.md`** — sobreescribir ENTERO. Es la ÚNICA narrativa de la sesión:
   - Fecha/hora de cierre · último commit
   - Qué se hizo (5-15 bullets) · en qué estado quedó (branch / build / dispositivos)
   - Lo que quedó en curso · **próximo paso CONCRETO** (nunca "seguir avanzando") · bloqueos
   - Archivos tocados · contexto importante que no esté en otros docs
3. **`docs/CHANGELOG.md`** — entrada nueva al tope (Added / Changed / Fixed / Removed según corresponda).
4. **`docs/TODO.md`** — tareas completadas AFUERA (la historia queda en CHANGELOG); nuevas con criticidad. El header es solo `YYYY-MM-DD — ver SESSION_HANDOFF.md`.
5. **Spec del fix**: si una fase de `docs/remediation/` quedó implementada en esta sesión → marcar el estado en su línea 1 y moverla a `docs/archive/`.
6. **Compilación real antes de cerrar**: si se tocó backend → `cd src-tauri; cargo check` (y `cargo clippy` si aplica). Si se tocó frontend → `node --check src/main.js`. Un cierre no declara "hecho" sin esto. Para lo visual/perf, dejar anotado en el HANDOFF qué se observó (Task Manager, panel de LOG).
7. **Checklist de regresión** — lo que la remediación toca y hay que cuidar de no re-romper: (a) consumo de CPU/RAM en reposo (Task Manager); (b) parpadeo de peers (que un peer real no aparezca/desaparezca solo); (c) transferencia PC→Android y Android→PC. Verificar solo los que esta sesión pudo haber tocado.
8. **Consistencia de números**: verificar que ningún dato quedó duplicado en dos docs — cada número vive solo en su fuente única.
9. **Memoria**: si cambió un hecho estructural (de cómo trabaja el usuario o del stack) → actualizar la memoria persistente de Claude.
10. **Commit + push**: `git add -A` → mostrar `git status` al usuario → commit `docs: cierre de sesión YYYY-MM-DD — <tema>` → push **solo si el usuario lo pidió** (regla del proyecto: no push sin permiso).
11. **Reporte final**: SHA · archivos actualizados · próximo paso al retomar · bloqueos · **"Chat listo para descartar — la próxima misión arranca con /inicio en un chat nuevo."**

## Edge cases

- Cambios sin commitear que el usuario NO quiere commitear → preguntar: stash / descartar / commit selectivo / dejar sin commitear y anotarlo en el HANDOFF.
- Drift entorno↔repo sin resolver → documentarlo en el HANDOFF como PRIMER paso de la próxima sesión.
- Push falla por conflicto remoto → pull + merge; si el conflicto no es trivial, parar y avisar antes de seguir.
