# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-21 · **Branch**: `feat/displays` · **Working tree**: limpio (tras el commit de cierre).

## En una línea

**La Fase 3 del SPEC-displays está IMPLEMENTADA y verificada LOCAL, pero NO probada en hardware todavía.**
Entraron las cuatro cosas de la fase: **perfiles** guardados (guardar/cargar/borrar), **ajustes**
(plazo del auto-revert), **watcher en vivo** (`WM_DISPLAYCHANGE` refresca la lista sola sin apretar
REFRESH) y el **lienzo de arrastre** (opción A, espejo de Windows: se pegan al borde, APLICAR es un
`SetDisplayConfig` por la red de auto-rollback). Todo pasó el gate local y **dos rondas de revisión
adversarial** (5 hallazgos, los 5 corregidos). Falta lo que no puedo hacer yo: **Guido probándolo en
el desktop de 3 pantallas** (ver "Próximo paso").

## Lo que se hizo

- **Perfiles** (`displays/mod.rs` + 4 comandos en `lib.rs`): cablea el motor del crate puro que ya
  existía (`save_profile`/`apply_profile`/`list_profiles`/`delete_profile`) sobre el store atómico de
  la Fase 2. **Cargar un perfil pasa por la MISMA red que el detach** (watchdog + auto-revert). **No
  hubo migración**: el JSON ya tenía el campo `profiles` (vacío) desde la Fase 2.
- **Ajustes** (2 comandos): editar el plazo del auto-revert. Al guardar se cambia SOLO ese campo y se
  preserva el resto de `AppSettings` intacto (atajos, etc. — features de Monarch que Millennium no usa).
- **Watcher `WM_DISPLAYCHANGE`** (`displays/system_events.rs`): la ventana oculta de la Fase 2 ahora
  atiende también el cambio de topología, por un **canal SEPARADO** del resume. **Refresca la vista
  pero NO invalida el cache** (invalidar borraría el recuerdo del monitor detachado — solo el resume
  invalida). Por evento, sin poll → CPU en reposo intacto por construcción.
- **Lienzo de arrastre** (`aplicar_layout` en `mod.rs` + comando `displays_apply_layout` + ~250 líneas
  de canvas en `main.js`): arrastrás rectángulos a escala, se **pegan al borde** (snap), y APLICAR
  manda las posiciones. El backend matchea por `(adapterLuid, targetId)` —no por EDID—, ancla el
  primario en `(0,0)`, y aplica por la red compartida. Staged: nada se toca hasta APLICAR.
- **La red compartida `aplicar_con_red`**: la usan cargar-perfil y el lienzo. **No compara ids a ese
  nivel a propósito** (ADR-012): la verificación por re-enumeración la hace el backend (`settle_poll`);
  el auto-revert real es el watchdog.
- **2 rondas de revisión adversarial** (workflows). A/B/C: 3 hallazgos (mi verificación al cargar
  perfil comparaba ids de dos enumeraciones distintas → falsos positivos y negativos; y un detalle de
  mayúsculas al pisar). Lienzo: 2 hallazgos (el borrador se pisaba con lo viejo durante la cuenta
  regresiva; y al re-entrar a la pestaña se perdía un acomodo sin aplicar). **Los 5 corregidos.**

## En qué estado quedó

- **Build local (scratch de displays)**: verde en las DOS ramas (Windows + no-Windows/gate Android),
  sin advertencias. `displays-tests` 13/13, `vendor/monarch` 22/22, `node --check` OK.
- **CI**: NO corrido — **no se pusheó** (regla del proyecto: push solo si Guido lo pide). El `.exe`
  para probar sale del CI, así que el push es el que lo genera.
- **Hardware**: NO verificado. Es el corazón del próximo paso.
- **Whole-crate `cargo check`**: sigue roto local (dlltool, documentado). "Compila" el binario entero
  solo lo afirma el CI; el módulo displays se verifica con el scratch (ver DECISIONS).

## Próximo paso CONCRETO (al retomar)

1. **Pushear `feat/displays`** (dispara el CI que produce el `.exe`). Ojo: hoy el CI corre incluso en
   pushes de solo-docs (tarea abierta en TODO).
2. **Smoke en el desktop de 3 pantallas** con ese `.exe` — la lista de evidencia que pidió Guido:
   - guardar un perfil y volver a cargarlo;
   - cambiar el plazo del auto-revert desde AJUSTES y ver que el próximo cambio use el nuevo;
   - enchufar/desenchufar algo y ver la LISTA actualizarse sola **sin apretar REFRESH**;
   - acomodar los monitores en el LIENZO, APLICAR, confirmar, y que quede persistido (sobrevive
     reiniciar);
   - que un layout malo del lienzo **vuelva solo** si no confirmás (igual que el detach de la TV).
3. **Regresión** (criterio #1): transferencia/clipboard/discovery siguen igual; **CPU en reposo ~0%
   en el Task Manager** (el watcher es por evento, pero es argumento hasta verlo).

## Bloqueos

Ninguno técnico. Lo único pendiente es la verificación física, que depende de Guido + el `.exe` del CI.

## Archivos tocados

`src-tauri/src/displays/mod.rs` (DTOs + métodos perfiles/ajustes/lienzo + `aplicar_con_red`),
`src-tauri/src/displays/system_events.rs` (WM_DISPLAYCHANGE, 2º canal), `src-tauri/src/lib.rs`
(7 comandos nuevos + registro), `src/index.html` (pestañas + panes + lienzo), `src/main.js`
(pestañas, perfiles, ajustes, canvas), `src/styles.css` (estilos de todo lo nuevo).

## Contexto que no está en otros docs

- **El crate scratch para verificar displays local** (receta en DECISIONS): al reconstruirlo, **sacar
  `winreg`** de las deps windows — arrastra `windows-sys → windows_x86_64_gnu`, cuyo build-script pide
  `dlltool` (ausente acá). displays no usa winreg; `windows 0.60` es raw-dylib y `cargo check` no lo
  pide. Anotado en DECISIONS (Nota de verificación).
- Decisiones técnicas nuevas: **ADR-012** (por qué el apply de layout completo no compara ids en la
  glue) y **ADR-013** (el watcher de dos canales: refresca sin invalidar).
