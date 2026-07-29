# SPEC: Cosmética final del combo Millennium (v1.5.2)
Los últimos cabos que quedaron dando vueltas tras la partición, todos juntos en un release chico: el updater apunta al nombre nuevo, y el logo de la app deja de decir "CLIPBOARD".
- Estado: ✅ IMPLEMENTADO 2026-07-29
- Fecha: 2026-07-29

## Por qué (el dolor)
Tras la partición quedaron 2 cabos cosméticos en el combo: (1) el updater todavía apunta al repo viejo `Millennium-Clipboard` — funciona por el redirect permanente, pero no es prístino; y (2) el logo de la app todavía muestra "CLIPBOARD" como sub-nombre (`◣◢ MILLENNIUM · CLIPBOARD // v1.5.1`), cuando el producto ahora es "Millennium" a secas. Salen los dos juntos en un release chico.

## Contexto (real, verificado)
- `src-tauri/src/updater.rs:15` → `const REPO = "guidocameraeq/Millennium-Clipboard"`. Hoy llega a Millennium por el redirect 301 (verificado: ve v1.5.1).
- `src/index.html:34` → `<span class="logo-sub">CLIPBOARD // <span id="hud-version">v0.0</span></span>`. El wordmark de arriba (`logo-text`, línea 33) ya dice `MILLENNIUM`; el sub dice `CLIPBOARD`.
- El título de ventana ya es "Millennium // GRID" (rebrand de Fase 1). El binario `millennium-clipboard.exe` y el identifier `com.guidocameraeq.millennium` NO se tocan.

## MODIFICA (con su efecto colateral)
- **`updater.rs:15`**: `const REPO` → `"guidocameraeq/Millennium"`. Efecto: el updater le pega **directo** al repo nuevo, sin depender del redirect. **Auto-curativo**: el arreglo viaja adentro del update que se instala por el redirect que hoy funciona; al instalarlo, la app queda apuntando directo. **Sin ventana de riesgo** (nadie baja nada a mano; el que no actualiza sigue por el redirect permanente).
- **`index.html:34`**: `logo-sub` `CLIPBOARD // vX` → **`GRID // vX`** (matchea el título de ventana; el `#hud-version` con la versión no se toca). Efecto: el logo pasa a leer `◣◢ MILLENNIUM · GRID // v1.5.2`.
- **Version bump** `1.5.1` → **`1.5.2`** en los 3 archivos (Cargo.toml, tauri.conf.json, Cargo.lock) para el release.

## NO SE TOCA
- El binario `millennium-clipboard.exe`, el identifier `com.guidocameraeq.millennium`, el zombie-killer, el asset del release, el motor de transferencia, el protocolo y TODA feature.
- Los dos repos ya pulidos (README, landing, About) y la partición.
- El redirect `Millennium-Clipboard → Millennium`: sigue vivo; el flip solo le deja de depender.
- **Android** (app_name, notificación del servicio, nombre del `.apk`): **DIFERIDO** — usa otra tubería de build (el `.apk` se compila aparte con keystore, NO viaja en el release del `.exe`). Se hace cuando toque compilar un APK.
- El repo/identifier del clipboard-only (`MillenniumClipboard`).

## Criterios de aceptación (verificables)
- **(Regresión)** La app diaria sigue abriendo, funcionando y actualizándose; el updater **NO se rompe** en la transición.
- CUANDO se saca `v1.5.2`, la app instalada la DEBE ofrecer y aplicar por el **updater normal** (vía redirect), **sin descarga manual**.
- Tras instalar `v1.5.2`: el commit taggeado DEBE tener `updater.rs:15 == "guidocameraeq/Millennium"` (inspección en GitHub), y el logo DEBE leer `MILLENNIUM · GRID // v1.5.2` (sin "CLIPBOARD").
- El binario, el identifier y todo lo de NO SE TOCA quedan intactos.

## Supuestos
- [BAJO] El sub-wordmark queda `GRID // vX` (matchea la ventana). Si Guido prefiere solo la versión, o `MILLENNIUM` pelado, es una palabra distinta.

## Riesgos y decisiones ⚠️
- ⚠️ **Requiere release (v1.5.2)**: los dos cambios viven en la app y surten efecto al instalar. Consecuencia: hay que taggear y que Guido instale. **Sin riesgo de updater** — es auto-curativo por el redirect permanente (razonamiento verificado con Guido).
- ⚠️ **Android diferido** (decidido): el nombre en Android sigue diciendo "Millennium Clipboard" hasta que se compile un APK nuevo. Consecuencia: inconsistencia solo en el celular (que no es el driver diario).
