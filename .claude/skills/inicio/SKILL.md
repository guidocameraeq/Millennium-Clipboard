---
name: inicio
description: Ritual de inicio de sesión de Millennium Clipboard. Usar cuando el usuario dice "inicio", "retomemos", "arranquemos", "¿en qué quedamos?" o al empezar una sesión de trabajo nueva.
---

# /inicio — arrancar una sesión de trabajo

El hook SessionStart ya inyectó `SESSION_HANDOFF.md` + cabecera del TODO + estado git al abrir este chat. **NO los releas** (es costo puro de contexto). Releé el archivo solo ante señal de drift: fecha vieja, contradicción con git, o si el hook no aparece en el contexto.

## Pasos

1. Cruzar handoff vs realidad: ¿el "último commit" del handoff coincide con `git log`? ¿Hay working tree sucio que el handoff no menciona? Detectar también: fases del spec implementadas sin archivar, TODO que contradice al handoff, drift repo↔entorno anotado.
2. Confirmar al usuario en 3 líneas: **Estado** / **Próxima acción** (del handoff) / **Bloqueos**. Si no dio la misión, preguntarla.
3. **NO tocar código ni docs hasta el OK explícito del usuario.**
4. Con la misión confirmada, y SOLO si la misión toca el backend: correr `cd src-tauri; cargo check` — no-bloqueante (si falla, reportarlo y seguir; el inicio nunca se traba por esto).

## Reglas

- **Una sesión = una misión.** Si a mitad de sesión aparece una misión distinta de verdad, proponer `/cierre` y chat nuevo.
- Si el handoff tiene más de 7 días, avisar que puede estar viejo y verificar contra git antes de confiar en él.
- Si la misión es "ejecutar el fix": una **fase por vez**, en orden (empieza por `docs/remediation/windows/phase-0-stop-the-bleed.md`), leyendo antes `docs/remediation/00-SHARED-CONTEXT.md`. Android requiere decidir "núcleo headless vs foreground-only" (`docs/remediation/android/SPEC.md`) **antes** de tocar código.
