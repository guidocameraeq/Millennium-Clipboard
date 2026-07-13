---
name: smoke
description: Smoke test de Millennium Clipboard — probar de verdad, con evidencia. Usar cuando el usuario dice "smoke", "probá que funciona", "verificá", o antes de declarar una fase/cambio como hecho.
---

# /smoke — probar de verdad, con evidencia

Nunca declarar "listo" sin esto. **"Apliqué el cambio" ≠ "funciona".** Un bug se declara resuelto solo cuando el caso que lo disparó se reprodujo y ya no falla. Si un paso no se pudo verificar con evidencia → se reporta como **NO VERIFICADO**, jamás como hecho.

## Parte 1 — Gate de compilación (determinista, siempre)

Corré lo que aplique a lo que tocaste, desde la raíz del proyecto:

1. **Backend Rust** (si se tocó `src-tauri/`):
   - `cd src-tauri; cargo check` — que compile limpio.
   - `cd src-tauri; cargo clippy` — sin warnings nuevos.
   - `cd src-tauri; cargo test` — si la fase agregó tests (`#[cfg(test)] mod tests`). Hoy no hay suite general; se corre solo si existen.
2. **Frontend** (si se tocó `src/`): `node --check src/main.js` — sintaxis OK.
3. **Android** (si se tocó `src-tauri/gen/android/` o Rust bajo `#[cfg(target_os="android")]`): `npm run tauri android build --apk` — es la única forma de verlo compilar de verdad (`cargo check` compila para el host, no para Android).

Reportá cada uno con su salida real (verde / el error). Es el mínimo obligatorio: **ningún cambio se da por hecho sin al menos esta parte**.

## Parte 2 — Prueba de comportamiento (cuando el cambio es de cara al usuario)

Esto no se automatiza: necesita la app corriendo y, para transferencias, un segundo dispositivo. Definí el caso ANTES de probar (qué flujo, qué entrada, qué se espera) y verificá el **efecto**, no solo que "respondió":

- **Consumo (Fase 0):** app corriendo en reposo → mirar CPU y RAM en el **Task Manager**. El objetivo de la Fase 0 es que en reposo el consumo sea bajo. Anotar los números observados (antes/después si se puede).
- **Peers (Fase 1):** con un peer real en la red, que aparezca y **no parpadee** (no desaparecer/reaparecer solo).
- **Transferencia:** mandar texto y un archivo **PC→Android** y **Android→PC** → que llegue completo y quede en el destino correcto (p. ej. `/Downloads`).
- **Panel de LOG:** que no crezca sin control ni trabe la UI.

## Reporte (formato fijo)

**Estado inicial** → **Gate de compilación** (cada comando + salida real) → **Prueba de comportamiento** (caso + qué se observó + evidencia: números del Task Manager, captura, log) → **Fixes aplicados** (si hubo) → **Re-verificación** tras el fix.

Con datos reales, no con resúmenes. Lo que no se verificó, va como **NO VERIFICADO**.
