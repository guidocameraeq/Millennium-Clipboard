# Spec de remediación — Windows (desktop)

> Requiere haber leído `../00-SHARED-CONTEXT.md`. Ejecutar las fases **en orden**; cada una es shippable por sí sola.

## Meta del spec

Que la versión de escritorio pase de "anda pero consume una barbaridad y es buggy" a **optimizada, estable y con un modelo de confianza real**. La mayor parte del problema de rendimiento se resuelve en la Fase 0.

## Fases

| Fase | Archivo | Qué resuelve | Esfuerzo | Riesgo |
|---|---|---|---|---|
| **0** | [`phase-0-stop-the-bleed.md`](phase-0-stop-the-bleed.md) | Las 3 causas verificadas de CPU/RAM + `[profile.release]` + autostart roto | ~1-2 días | Bajo |
| **1** | [`phase-1-discovery.md`](phase-1-discovery.md) | El parpadeo de peers: consolidar los 3 mecanismos, aplicar la corrección de IP, sacar el filtro /24 (Rust compartido con Android) | ~3-5 días | Medio |
| **2** | [`phase-2-correctness.md`](phase-2-correctness.md) | Favoritos que desaparecen (stores no atómicos), envíos al dispositivo equivocado, texto recibido que se pierde, y otros bugs de UI | ~2-3 días | Bajo-Medio |
| **3** | [`phase-3-security.md`](phase-3-security.md) | Pinning real del certificado TLS, CSP, escaping de strings de peers, gate de `/text`, verificación del updater | ~2 días | Medio |

> **Código compartido:** la Fase 1 (descubrimiento) y la Fase 3 (pinning) tocan Rust que también usa Android. Al hacerlas quedan resueltas para ambas plataformas; el spec de Android las referencia como prerrequisito, no las duplica.

## Paso 0 — medir la línea de base (hacer ANTES de tocar nada)

Para poder demostrar la mejora, capturá el consumo actual:

```powershell
# Con la app corriendo y una captura de pantalla (imagen) en el portapapeles,
# minimizada a la bandeja, sin hacer nada:
Get-Process | Where-Object { $_.MainWindowTitle -like '*Millennium*' -or $_.ProcessName -like '*millennium*' } |
  Select-Object ProcessName, Id, CPU, @{N='RAM_MB';E={[math]::Round($_.WorkingSet64/1MB,1)}}
# Anotá: %CPU en reposo, RAM, y cómo crecen tras 30-60 min de uptime.
# El proceso del WebView (msedgewebview2) es donde se ve el costo del frontend.
```
Guardá esos números. Después de la Fase 0, en reposo el CPU debe quedar **cerca de 0%** y la RAM debe **dejar de crecer**.

## Criterios de aceptación del spec completo (Definition of Done)

- [ ] **Reposo:** con la app minimizada y una imagen en el portapapeles, el CPU en reposo es ~0% y estable (no hay un core al 20-100%).
- [ ] **Memoria:** tras horas en la bandeja, la RAM del proceso y del WebView no crece de forma monótona.
- [ ] **Peers estables:** un peer online no parpadea (no aparece/desaparece); cambiar de IP el peer no lo tira por más de un ciclo.
- [ ] **Sin pérdida de datos:** los favoritos/ajustes sobreviven a un cierre abrupto; el texto recibido no se destruye solo.
- [ ] **Envío correcto:** nunca se manda al peer equivocado por un re-render.
- [ ] **Seguridad:** una máquina en la LAN que se hace pasar por un peer de confianza (mismo fingerprint en `/info`, cert distinto) es **rechazada**.
- [ ] **Build:** `npm run tauri build` produce un `.exe` de ~8-12 MB (con `[profile.release]`), y el autostart apunta al ejecutable actual.
- [ ] Compila limpio (`cargo check`, `cargo clippy` sin warnings nuevos) y arranca sin panics (revisá `crash.log`).

## Notas de riesgo

- La Fase 1 cambia lógica de red viva; testeala con al menos 2 dispositivos y observá el panel de LOG (que la Fase 0 ya dejó acotado).
- La Fase 3 cambia el handshake TLS: verificá que dos peers ya emparejados se siguen viendo y transfiriendo **después** del cambio (no romper compat con peers viejos que aún usan el esquema self-reported: definí la estrategia de transición en esa fase).
