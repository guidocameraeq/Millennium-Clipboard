# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-15 · **Último commit de código**: `5bb57e4` (bump 0.16.0 — esta sesión **NO tocó código**, solo docs). **Working tree**: docs nuevos/actualizados (SPEC-ui-polish + HANDOFF/CHANGELOG/TODO). Sin push (nadie lo pidió).

## Qué se hizo (sesión de verificación + auditoría + spec)

**Fase 2 — Bloque A (Datos): VERIFICADO FÍSICAMENTE ✅** — probado con una **instancia de prueba aislada** (`MILLENNIUM_INSTANCE=verif`, `MILLENNIUM_PORT=53400`), **sin tocar los datos reales** del usuario:
- **P1 — favorito sobrevive a un crash**: se favoriteó DRACOSSSLAPTOP, se mató el proceso **de golpe** (`Stop-Process -Force`), el archivo quedó **íntegro** (sin `.tmp` residual → escritura atómica OK), y al reabrir `[prefs] loaded 1 favorite(s)` + el favorito visible en pantalla.
- **P2 — JSON corrupto**: se corrompió `prefs-verif.json` a mano → al reabrir saltó `ERR [jsonstore] parse failed`, se creó `prefs-verif.json.corrupt` con el contenido recuperable, y arrancó con 0 favoritos (default). **2 matices honestos** (ninguno rompe el criterio): el archivo original queda corrupto hasta el próximo write (el `.corrupt` preserva el dato; sin loop dañino) y `prefs` no muestra aviso visual de corrupción (solo el log) — anotado como posible mejora en TODO 🟢.

**Fase 2 — Bloque B (UI): 1/5 ✅** — "texto entrante sobrevive un ACK" verificado (los carteles `#incoming-toast` y `#toast` conviven, ninguno se borra). **Faltan 4** (necesitan 2 PCs): TARGET LOST, error que no se pisa a los 5 s, barras TX/RX independientes, rename que sobrevive un `peers-changed`.

**Auditoría de UI** (workflow, 30 agentes: 4 dimensiones + verificación adversarial + consolidación) → **18 hallazgos verificados** contra el código.

**SPEC de pulido de UI** creado y **READY** → `docs/SPEC-ui-polish.md`. 6 tareas (T1 recortes de info, T2 bugs del flujo de archivos, T3 UX chicos, T4 rediseño de la cola, T5 Config colapsable, T6 contraste con preview), **NO SE TOCA** explícito (motor de transferencia, render por diff, escaping, CSP+pinning de Fase 3, backend, datos), 9 criterios verificables, y 3 decisiones del dueño resueltas.

## Estado

- **Branch**: `main`. **Código (`src/`, `src-tauri/`): INTACTO** — no se tocó nada, no hay build que verificar.
- **App real**: corriendo en `:53319` (PID vivo), reabierta tras las pruebas. **Datos reales intactos** (`prefs.json` con los 3 favoritos: LOCALHOST, DRACOSSSLAPTOP, DRACO-PC).
- **Instancia de prueba y sus archivos (`*verif*`)**: borrados. Nada de prueba quedó en disco.

## Próximo paso CONCRETO

**Ejecutar el SPEC de pulido de UI en un chat NUEVO**: abrir Claude Code en esta carpeta y pegar `inicio — ejecutá el spec docs/SPEC-ui-polish.md (está READY)` + correr `/smoke` al terminar (el criterio #1 es la regresión: enviar/recibir sigue andando y sin errores de CSP). El dueño va a acompañar los tramos visuales (T6 contraste tiene gate de preview).

**Aparte** (no bloquea): cerrar las **4 pruebas de UI de Fase 2** (Bloque B) con las 2 PCs.

## Bloqueos

- Ninguno.

## Archivos tocados

- **Nuevo**: `docs/SPEC-ui-polish.md` (spec READY).
- **Docs de cierre**: `docs/SESSION_HANDOFF.md`, `docs/CHANGELOG.md`, `docs/TODO.md`.
- **Código**: NINGUNO.

## Contexto importante (para la próxima sesión)

- **Cómo probar con 2 instancias**: en una **misma PC NO corren 2 instancias** — el plugin `single-instance` 2.4.2 usa un lock por *identifier*, así que la 2ª copia se cierra sola y enfoca la 1ª (verificado en vivo). Para probar UI de a 2: usar **2 PCs**, o cerrar la app real y correr **UNA** instancia de prueba aislada con `MILLENNIUM_INSTANCE=<n>` **+** `MILLENNIUM_PORT=<p>` (el puerto NO se deriva del INSTANCE; son dos variables). El zombie-killer se saltea solo cuando `MILLENNIUM_INSTANCE` está seteada. Ver memoria `millennium-testing-2-instancias`.
- **Para TARGET LOST hace falta un peer NO favorito**: un favorito que se apaga queda como "PEER OFFLINE" (sigue en la lista); solo un peer **común** que desaparece dispara TARGET LOST. `PEER_TTL = 15 s` (3× el broadcast de 5 s). DRACOSSSLAPTOP es favorito.
- **El SPEC-ui-polish difiere 2 hallazgos** (conteo de peers repetido + navegar la lista con teclado) porque tocan el render por diff (zona protegida) — decisión D3 del dueño: van a un spec chico aparte (anotado en TODO 🟢).
