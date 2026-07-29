# SPEC: Partir el repo en dos — "Millennium" (combo) + "Millennium Clipboard" (limpio)
Partir el repo actual en dos productos: el combo (Clipboard + Displays) renombrado a "Millennium", y un repo nuevo y limpio solo-clipboard `MillenniumClipboard` (producto "Millennium Clipboard"). Puro repo/branding/plumbing de releases: NO cambia ninguna feature.
- Estado: ✅ IMPLEMENTADO 2026-07-29
- Fecha: 2026-07-28

## Por qué (el dolor)
Hoy un solo repo (`guidocameraeq/Millennium-Clipboard`) mezcla dos productos: el clipboard solo y el combo con Displays. Guido quiere (1) un repo limpio "Millennium Clipboard" con SOLO el motor de portapapeles y su landing/README, y (2) el combo como proyecto aparte llamado "Millennium", con su propia landing/README — **sin romper su instalación de todos los días ni el updater**. Es puro plumbing: cero features nuevas o cambiadas.

## La decisión de fondo (por qué NO hay baile de transición)
El combo (la app diaria de Guido) **se queda en el repo de hoy, solo renombrado** a `Millennium`. Al repo clipboard nuevo le damos un nombre **distinto** (`MillenniumClipboard`, sin guion) → **nunca se reusa el nombre viejo** → el redirect de GitHub del nombre viejo hacia `Millennium` **queda vivo para siempre** y la app instalada de Guido lo sigue solo. Sin update de transición obligatorio, sin checkpoint irreversible, sin ventana peligrosa. (Verificado con el red-team: reusar el nombre exacto era lo que metía una puerta de un solo sentido con falla silenciosa; un nombre distinto la elimina de raíz.)

## Contexto del código (verificado hoy, nombres/hechos REALES)
- **Updater = una sola dirección clavada**: `src-tauri/src/updater.rs:15` → `const REPO = "guidocameraeq/Millennium-Clipboard"`. Es el **único** acople funcional del binario al repo. Usa `reqwest`, que **sigue redirects por default en el mismo host** → tras el rename, el updater del combo (const viejo) VE las releases vía redirect de la API (`api.github.com/repos/VIEJO/...` → 301/307 → `.../Millennium/...`). Eslabón verificado y sólido.
- **CI dinámico**: `.github/workflows/release.yml` usa `${GITHUB_REPOSITORY}` → se auto-adapta al renombre y es **portable** tal cual al repo clipboard.
- **Corte limpio en v1.0.0** = commit `5ffdfca`: clipboard puro (sin `src-tauri/src/displays/`, sin `src-tauri/vendor/monarch/`, README sin una sola mención de displays/monitores), **ancestro lineal directo de `main`** (41 commits después; Displays entró en `76afb8a`). Extraerlo = `git push` del commit (arrastra su historia hasta v1.0) — **sin reescribir nada**.
- **Clave (verificado)**: entre v1.0 y hoy el clipboard NO cambió — el código solo CRECIÓ (2.814 líneas agregadas, 21 borradas) y TODO lo nuevo fue Displays; cero feat/fix del clipboard/transfer. O sea: **cortar en v1.0 = el clipboard de HOY, sin Displays** (no se lleva una versión vieja).
- ⚠️ **`5ffdfca` NO tiene los workflows del CI**: `.github/workflows/` está vacío ahí; se agregaron después (en `0766749`). El repo clipboard **necesita que se le porten**.
- ⚠️ **En `5ffdfca` el const del updater dice `Millennium-Clipboard`**: el repo clipboard, al llamarse `MillenniumClipboard`, necesita que se le **edite el const a `guidocameraeq/MillenniumClipboard`** para que SUS propios updates funcionen.
- **Landing**: vive en la rama **`gh-pages`** de este repo (un solo commit `c44fe99`, un `index.html`, bilingüe EN/ES) → sirve `guidocameraeq.github.io/Millennium-Clipboard`. **VERIFICADO: es 100% CLIPBOARD** (título "Millennium Clipboard", 0 menciones de Displays/monitores/perfiles/pantallas — las "23" que había contado antes eran `display:` del CSS). O sea **sirve TAL CUAL para `MillenniumClipboard`** (solo se copia y se revisan los links). NO viaja con el `git push` de `main` (es otra rama). **Corolario**: la que quedará floja es la landing del COMBO (hoy no vende Displays) — contenido nuevo, aparte, no bloquea la partición.
- ⚠️ **GitHub Pages NO redirige tras un rename**: `guidocameraeq.github.io/Millennium-Clipboard` da **404** tras el rename (la app NO usa la landing, usa la API — esto es solo UX/links externos).
- **La app diaria de Guido = el combo** (usa Displays). Identifier `com.guidocameraeq.millennium`; binario `millennium-clipboard.exe`; ventana "Millennium Clipboard // GRID". El zombie-killer matchea por nombre de proceso; el asset-picker prefiere `*portable.exe` y si no cualquier `*.exe`.

## AGREGA (lo nuevo)
- Un repo GitHub nuevo `MillenniumClipboard` con: historia hasta `v1.0.0`, el const del updater editado a sí mismo, workflows del CI portados, la landing existente (ya 100% clipboard) copiada a su `gh-pages`, README de clipboard (viaja del commit), y su release funcionando.
- Un **ADR-003** en el hub que documenta la partición y deja la doc consistente con **3 repos**.

## MODIFICA (lo existente que se toca — con su efecto colateral)
- **Repo actual → renombrado en GitHub** `Millennium-Clipboard` → `Millennium`. Efecto: la app instalada de Guido sigue actualizándose por el redirect de la API; el CI se auto-adapta. La landing del combo se muda a `.../Millennium`; **la URL vieja de Pages da 404** hasta que se actualicen los links.
- **README + landing del combo**: pasan a decir "Millennium" y apuntan a las URLs nuevas. Efecto: cosmético, in-repo.
- **Nombre VISIBLE del combo** (`tauri.conf.json`: `productName` + `title` de la ventana) → "Millennium". Efecto: **SEGURO/cosmético** — es solo el nombre que se VE en la ventana. NO cambia el binario `millennium-clipboard.exe` (el que tocaría updater + zombie-killer) ni el identifier (los datos en `%APPDATA%` cuelgan del identifier, no del nombre visible). Viaja en la release del debut de Millennium.

## NO SE TOCA (el seguro de no romper — criterio #1 del smoke)
- **La instalación diaria de Guido**: no se mueve, no se le reescribe historia. Solo se renombra el repo (metadata + redirect permanente).
- **El identifier `com.guidocameraeq.millennium` del combo**: intacto → `%APPDATA%`, autostart y single-instance de Guido **intactos**.
- **El binario `millennium-clipboard.exe` (el nombre del ARCHIVO), el zombie-killer y el asset del release**: intactos. *(El nombre que se VE en la ventana SÍ cambia a "Millennium" — cosmético; lo que NO se toca es el filename del `.exe`.)*
- **El const del updater DEL COMBO** (`guidocameraeq/Millennium-Clipboard`): **se deja como está** — sigue funcionando por el redirect permanente. (Flip a `Millennium` = higiene opcional futura, sin apuro, sin riesgo.)
- **El motor de transferencia, el protocolo y TODA feature**: intactos. Cero cambios de comportamiento.
- **La historia y las releases del combo**: se conservan enteras. No se borra, resetea ni force-pushea nada.
- **El nombre viejo `Millennium-Clipboard`**: NO se reusa jamás (por eso el redirect vive para siempre).

## Plan por fases (con checkpoints y OK de Guido)

**Fase 1 — Dejar Millennium completo y prolijo.**
- Renombrar el repo → `Millennium` en GitHub (instantáneo; la app instalada sigue por el redirect).
- Cambiar el nombre VISIBLE del combo a "Millennium" (`productName` + `title` de la ventana; el binario NO se toca).
- README del combo → "Millennium" + URLs nuevas. Landing del combo (`gh-pages`): renombrar la marca a "Millennium" y actualizar URLs. *(Nota: esta landing hoy habla SOLO de clipboard; sumarle la parte de Displays es contenido nuevo, aparte — no bloquea la partición.)*
- Sacar la release "debut de Millennium" que lleva todo esto. **Ojo con el número**: el combo está en 1.5.x → la release DEBE ser numéricamente mayor (ej. 1.5.x-final o 1.6.0), **NUNCA 1.0** (el updater vería 1.0 como downgrade y no se ofrecería). "Millennium 1.0" puede ser el nombre en la landing; el número interno sigue subiendo.
- **Verificar**: (a) la app instalada recibe la release y muestra "Millennium" en la ventana; (b) el updater devuelve 200 vía redirect; (c) la landing nueva `.../Millennium` abre y la vieja da **404 — esperado**. → **OK de Guido**.

**Fase 2 — Crear el repo clipboard limpio `MillenniumClipboard`.** *(Independiente de la Fase 1: se saca de un punto fijo de la historia —v1.0— que git preserva pase lo que pase con Millennium; el orden no arriesga nada del clipboard.)*
- Crear repo GitHub `MillenniumClipboard` (nombre distinto → nunca toca el redirect del viejo).
- `git push` del commit `5ffdfca` como `main` (arrastra su historia hasta v1.0). El README viaja (ya clipboard-puro).
- **Editar** `updater.rs:15` → `"guidocameraeq/MillenniumClipboard"` (para que sus propios updates funcionen).
- **Portar** los workflows del CI (`release.yml` + `build.yml`, portables por `${GITHUB_REPOSITORY}`).
- **Copiar la landing existente** (`gh-pages`, commit `c44fe99`) a la `gh-pages` del repo nuevo — ya es 100% clipboard, sirve tal cual (solo revisar que los links apunten a `MillenniumClipboard`). NO hay que crearla ni recortarla.
- **Pre-check**: confirmar que compila **verde en el CI de hoy** (el corte es de 41 commits atrás; las deps pudieron driftear). Si no, decidir con Guido: portar el fix mínimo o mover el corte.
- Tag `v1.0.0` (o `v1.0.1` si hay que retaggear tras portar workflows). Release por CI.
- **Verificar**: clona, compila verde, la release sale con su `.exe`, su updater apunta a sí mismo, la landing clipboard abre. → **OK de Guido**.

**Fase 3 — Doc del hub + links externos.** ADR-003 (la partición + por qué el camino sin baile) + reflejar "**3 repos**" (Millennium combo · MillenniumClipboard · Monarch) en la doc del hub (`../docs/DECISIONS.md`, `../CLAUDE.md`, `../SPEC-0.md` donde corresponda). Actualizar a mano cualquier link a la landing/URL vieja del combo. **Verificar**: doc consistente, sin links muertos. → **OK de Guido**.

**(Opcional, higiene, sin apuro y sin riesgo)** — En algún update NORMAL futuro del combo, meter de paso el flip del const a `guidocameraeq/Millennium` para no depender del redirect indefinidamente. Si se hace: recordar que el parser del updater ignora el sufijo `-beta` (la versión tiene que ser numéricamente mayor). Si nunca se hace, el redirect sigue cubriendo — no se rompe nada.

## Criterios de aceptación (binarios, verificables)
- **(Regresión, #1)** En CADA fase, el combo instalado de Guido: **abre**, muestra su versión en Settings, y **hace una transferencia de prueba a un peer OK**.
- CUANDO se renombra el repo (Fase 1), el updater de la app instalada DEBE devolver 200 y listar releases vía redirect de la API (verificable sin tocar la app).
- El repo `MillenniumClipboard` DEBE contener SOLO clipboard (sin `displays/` ni `vendor/monarch/`), con historia hasta `v1.0.0`, const del updater apuntándose a sí mismo, workflows portados, landing solo-clipboard, y una release con `.exe` que compiló verde.
- El identifier `com.guidocameraeq.millennium` del combo DEBE quedar intacto (los datos de Guido en `%APPDATA%` no se tocan).
- El nombre `Millennium-Clipboard` NO DEBE ser reusado por ningún repo (el redirect del combo se mantiene vivo).
- La doc del hub DEBE quedar consistente con 3 repos (ADR-003 presente) y sin links muertos a la landing vieja.

## Supuestos
- [ALTO] La app diaria de Guido es el combo. Con este camino, **no necesita instalar nada especial** — sigue actualizándose como siempre por el redirect.
- [BAJO] El combo queda apoyado en el redirect de GitHub indefinidamente. El redirect de repos es **permanente mientras no se reuse el nombre** (no lo reusamos). El flip del const es higiene opcional.
- [BAJO] El identifier del clipboard-only queda igual al del combo por ahora; si algún día Guido quiere tener los dos instalados a la vez, hay que darle uno propio (no afecta los datos del combo).

## Riesgos y decisiones ⚠️
- ⚠️ **Marca: repo + landing + nombre VISIBLE de la ventana** (revisado con Guido). El binario `millennium-clipboard.exe` NO se renombra (eso sí tocaría updater + zombie-killer + asset). Consecuencia: el `.exe` por dentro sigue diciendo "millennium-clipboard", pero eso solo se ve en el explorador de archivos.
- ⚠️ **El combo NO baja a "versión 1.0"**: está en 1.5.x y el updater solo ofrece números mayores; una 1.0 se vería como downgrade y no se instalaría en la app de Guido. El "1.0 desde cero" es del clipboard. El combo sigue su numeración (1.5.x-final / 1.6.0).
- ⚠️ **GitHub Pages no redirige**: la landing vieja del combo da 404 tras el rename (no es un bug). Links externos a esa URL mueren → se actualizan a mano en Fase 3.
- ✅ **Eliminado**: el riesgo de la puerta irreversible / orfandad silenciosa. Al NO reusar el nombre viejo, no existe punto de no retorno ni falla silenciosa del updater.
