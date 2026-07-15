# SPEC: Pulido de UI — Millennium Clipboard
Sacar ruido visual, arreglar 2 bugs del flujo de archivos y reorganizar la config, sin tocar el motor de transferencia ni la seguridad.
- Estado: READY
- Fecha: 2026-07-15

## Por qué (el dolor)
La app funciona y la estética le gusta al dueño, pero hay **demasiada información en pantalla** (relojes, contadores, jerga técnica) y el **flujo de archivos confunde**: al arrastrar un archivo no pasa nada visible y parece que se perdió. El panel de Configuración es una pila plana difícil de recorrer. Cuando esto esté, la UI se siente más limpia y el envío de archivos deja de asustar.

## Contexto del código (de la auditoría — archivo:línea REALES)
Frontend vanilla, sin framework ni bundler. 4 archivos: `src/index.html`, `src/styles.css`, `src/main.js` (~2240 líneas), `src/pre.js`. Hallazgos verificados:

- **Drag&drop** (`main.js:906-913`): el handler `tauri://drag-drop` encola con `addPathToQueue()` pero **nunca** setea `state.mode='file'`; en modo TEXT el panel `#mode-file` está `hidden` → no se ve nada y `transmit()` cae en texto vacío → "empty payload".
- **Cola de archivos** (`index.html:143` `<ul class="file-queue">`, `styles.css:983`, `renderQueue` en `main.js:825`): la lista es hermana **debajo** del dropzone, el texto "DROP FILES TO QUEUE" es fijo, y la lista no tiene `max-height`/scroll → con varios archivos empuja el botón TRANSMIT.
- **Tamaño 0 B** (`main.js:893` empuja `size:0` fijo; `main.js:834` lo pinta con `formatBytes(0)='0 B'`): el frontend no sabe el peso real.
- **Botón quitar `[X]`** (`main.js:834`): es un `<a>` tipo link, sin aspecto de botón ni área de toque.
- **UPTIME** (`index.html:45-48` + ticker `main.js:344-352`, `setInterval` 1s; el nodo se toma en `main.js:101`).
- **Placeholder rotativo** (`main.js:355-361`): 5 frases; las líneas `359` y `360` son slogans/jerga ("NO CLOUD. NO ACCOUNT..." / "mDNS DISCOVERY · TLS PINNED...").
- **Contador CHARS + hint CTRL+ENTER** (`index.html:126-129`; constante y llamadas en `main.js:90,417-420,974,2183`): "0000 CHARS" con ceros a la izquierda + un hint que ya está en el placeholder.
- **Hex bajo "TRANSMIT TO"** (`index.html:105` `.target-hex`, lo llena `selectPeer` en `main.js:679`, se resetea en `main.js:2210`; estilo `styles.css:440`).
- **PROTO mDNS+HTTPS** (`index.html:189-192`, estático sin binding JS; `styles.css:2197-2199` oculta nth-child(4)/(5) en mobile).
- **Textos con "mDNS"/jerga en modales** (`index.html:86,315,335,525,534,536`): párrafos largos en agregar/olvidar peer y QR.
- **DATA DIR falsa** (`index.html:467-472`, id `settings-data-dir`): ruta hardcodeada `%APPDATA%\com.guidocameraeq.millennium`, sin binding (`openSettingsModal` nunca la popula).
- **Panel CONFIG** (`index.html:363-473`): `settings-body` apila 7 `h3.settings-section` (GENERAL/INTERFACE/TRANSFERS/NOTIFICATIONS/SYSTEM/UPDATES/DIAGNOSTICS) planas, la mayoría con 1 opción, sin colapso.
- **Auto-selección** (`applyPeers` `main.js:2220-2222`): auto-selecciona `state.peers[0]` ignorando el filtro (que arranca en FAVORITOS, `main.js:195`); si `peers[0]` no es favorito, queda "PEER LOCKED" apuntando a un equipo que no está en la lista visible.
- **Contraste bajo** (`styles.css:61`, `--text-dim`): etiquetas chicas casi ilegibles sobre negro. Tocarlo cambia el look que al dueño le gusta (por eso va con preview).
- **Teclado/foco** (`styles.css:1158` `.sound-toggle input{display:none}` saca del tab-order a TODOS los switches; sin `:focus-visible`; foco no entra a los modales `main.js:1237`).
- **Botón "CHECK FOR UPDATE"** (`index.html:459` vs reset en `main.js:1780`): el HTML dice "CHECK FOR UPDATE" y el JS lo deja como "CHECK" tras usarlo.

> **Diferido (fuera de este spec, ver FUERA de alcance):** conteo de peers repetido (`#peer-count`/`#filter-hint`/footer PEERS/FAV, escrito dentro de `renderPeers`) y navegación por teclado de la lista (`buildPeerItem`) — ambos tocan el render por diff (zona protegida).

## AGREGA (lo nuevo)
- **Foco visible** (`:focus-visible`) en botones, textarea y switches (reemplazar el `display:none` de los inputs de switch por ocultación accesible). [T3]
- **Secciones colapsables** en Config con `<details>/<summary>` (sin JS → compatible con la CSP). [T5]
- **Estado de la cola dentro del cuadro** ("N archivo(s) listo(s)") + contenedor con `max-height`+scroll. [T4]

## MODIFICA (lo existente que se toca — con su efecto colateral)

### T1 — Recortes de info (quirúrgico, sin riesgo)
Sacar/acortar ruido visual. **Efecto colateral clave: al borrar un nodo del HTML hay que borrar también su JS o tira error.**
- Quitar **UPTIME**: borrar `index.html:45-48` **y** el `setInterval` `main.js:344-352` (si no, el ticker tira error cada segundo).
- Quitar 2 **slogans** del placeholder: borrar `main.js:359-360` (dejar las 3 útiles).
- Quitar **PROTO mDNS+HTTPS**: borrar `index.html:189-192` + limpiar la regla CSS muerta `styles.css:2197-2199`.
- Quitar **contador CHARS**: borrar `index.html:126-127` + la constante y las llamadas en `main.js:90,417-420,974,2183`. Dejar el hint CTRL+ENTER una sola vez.
- Ocultar **hex del target**: `display:none` en `.target-hex` (`styles.css:440`). El hex sigue visible en cada peer de la lista (no se pierde desambiguación).
- Acortar **textos de modales** (`index.html:86,315,335,525,534,536`): una frase en criollo sin "mDNS"/jerga.
- Quitar **DATA DIR falsa** (`index.html:467-472`): borrar la fila (ruta hardcodeada que nunca se actualiza).

### T2 — Bugs del flujo de archivos (quirúrgico)
- **Drag&drop → modo FILE** (`main.js:906-913`): si cayó ≥1 archivo, activar modo FILE (mismo switch que el botón FILE, `main.js:803-815`). Ideal: extraer `activateMode('file')` y reusarlo.
- **"0 B"** (`main.js:834`): no mostrar "· 0 B" cuando el tamaño es 0 (el front no sabe el peso real).

### T3 — UX chicos (quirúrgico)
- Botón **quitar archivo** (`main.js:834`): `<a>[X]` → `<button aria-label="Quitar">` con área de toque razonable.
- **Foco/teclado** (`styles.css:1158,894`; `main.js:1237`): `:focus-visible` visible, ocultación accesible de switches, enfocar el 1er control al abrir cada modal.
- **Botón update** (`index.html:459` / `main.js:1780`): misma etiqueta en HTML y en el reset del JS.
- **Auto-selección al abrir** (D1, `main.js:2220-2222`): auto-seleccionar el primer peer **VISIBLE según el filtro actual** (no `state.peers[0]` a secas). Cuidado: `applyPeers` está cerca del render por diff — tocar SOLO la línea de auto-selección, no `renderPeers`.

### T4 — Rediseño de la cola de archivos (más grande, no toca protegido)
`index.html:143` + `styles.css:983` + `renderQueue` `main.js:825`: mostrar el estado DENTRO del cuadro, lista con `max-height`+scroll para no empujar TRANSMIT. HTML+CSS+JS coordinado. Absorbe el botón `[X]` de T3.

### T5 — Reorganizar Config en colapsables (más grande, no toca protegido)
`index.html:363-473`: reagrupar las 7 secciones en ~4 y volverlas `<details>/<summary>`. **Cuidado: mantener los `id` que lee el JS y NO cruzar la frontera `.desktop-only` (secciones ocultas en Android).**

### T6 — Contraste de etiquetas (D2 — con preview obligatorio)
`styles.css:61` `--text-dim`: subir el valor para legibilidad. **Gate visual: mostrar antes/después al dueño y aplicar SOLO si aprueba. Si no le gusta, se deja el valor actual y esta tarea se marca "no aplicada por decisión".**

## NO SE TOCA (obligatoria — el seguro de no romper)
- **Render de peers por diff**: `buildPeerItem` / `updatePeerItem` / `renderPeers` — la lógica de diff incremental NO se reescribe.
- **El `state` global único**: se le pueden AGREGAR campos, no reestructurar.
- **El escaping de datos de peer** (`textContent`/`createElement`): ningún recorte reintroduce `innerHTML` con strings que vienen de un peer.
- **Fase 3 — CSP** (meta `Content-Security-Policy` en `index.html`): no se agregan scripts/estilos inline que la violen (por eso Config usa `<details>`, sin JS). **Cert pinning**: no se toca.
- **Backend Rust entero**: intacto (esto es solo frontend).
- **Motor de transferencia** (transmit, streaming, resume, pooling): intacto — el fix de drag&drop solo cambia el modo de UI, no el envío.
- **Datos del usuario** (favoritos, alias, iconos, settings): no se tocan ni migran.

## Criterios de aceptación (verificables — el primero es de regresión)
1. **Regresión**: todo lo de NO SE TOCA sigue igual — enviar y recibir texto y archivos entre 2 peers funciona, la grilla se actualiza por diff, y la consola (F12) no muestra violaciones de CSP ni errores nuevos.
2. CUANDO arrastro y suelto un archivo, el sistema DEBE cambiar a modo FILE y mostrar la cola con el archivo.
3. CUANDO hay archivos en cola, el sistema NO DEBE mostrar "0 B".
4. Ningún recorte DEBE dejar código muerto que tire error en consola (chequear tras quitar UPTIME y CHARS).
5. El panel Config DEBE mostrarse en secciones colapsables, con los toggles andando y sin romper la frontera Android (`.desktop-only`).
6. La cola de archivos DEBE mostrar su estado dentro del cuadro y NO empujar el botón TRANSMIT (scroll interno).
7. Con teclado, DEBE verse el foco en botones y switches y DEBE poder abrirse/operar un modal.
8. CUANDO abro la app con el filtro en FAVORITOS, el equipo "apuntado" DEBE ser uno VISIBLE en la lista (nunca "PEER LOCKED" a uno que no se ve).
9. El contraste (T6) solo se aplica si el dueño aprobó el preview; si no, las etiquetas quedan como estaban.

## Supuestos
- [BAJO] El frontend va embebido en el `.exe`; para VER los cambios se corre en dev (hot-reload). No afecta el build de producción.
- [BAJO] Sacar el contador CHARS y el hex del target no rompe ninguna función (son solo visuales).
- [MEDIO] `<details>/<summary>` es compatible con la CSP actual (no requiere JS inline). *(Verificar en ejecución: criterio #1.)*

## Riesgos y decisiones ⚠️
- **Datos del usuario**: NO se tocan (es UI). No hay tabla cambio/preservo.
- **Plata / permisos**: no aplica.
- ✅ **D1 — Auto-selección** (#14): DECIDIDO → auto-seleccionar el primer peer VISIBLE según el filtro. Consecuencia: cambia qué peer queda apuntado al abrir (mejora: deja de "trabarse" a uno invisible).
- ✅ **D2 — Contraste** (#15): DECIDIDO → se incluye (T6) con **preview obligatorio**; si en la previa no gusta, se deja el look actual. Consecuencia: cambia el look → por eso el gate visual.
- ✅ **D3 — Zonas protegidas** (#5 conteo, #17 nav por teclado): DECIDIDO → **AFUERA** (diferidas a un spec chico futuro). Consecuencia: no se entra al render por diff en este spec; el conteo repetido y la nav por teclado quedan pendientes.

## FUERA de alcance (qué NO entra)
- Que el frontend conozca el peso real por archivo (requiere cambio de backend). Solo se oculta el "0 B".
- Cualquier rediseño del motor de peers/transferencia.
- **Conteo de peers repetido** (#5) y **navegación por teclado de la lista** (#17): diferidos a un spec futuro (tocan el render por diff — decisión D3).
