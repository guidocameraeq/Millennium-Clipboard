# TODO — Millennium Clipboard

> ÚNICA fuente de pendientes del proyecto. Completado → SE BORRA (la historia vive en CHANGELOG y git). Header de 1 línea, sin narrativa de sesión.

2026-07-13 — ver SESSION_HANDOFF.md

## 🔴 Crítico
- [ ] **Fase 1 Windows — consolidar descubrimiento / fin del parpadeo de peers** (`windows/phase-1-discovery.md`). **EMPEZAR ACÁ** (la Fase 0 quedó completada y verificada 2026-07-13, archivada en `docs/archive/`).

## 🟠 Importante
- [ ] Fase 2 Windows — correctness: pérdida de datos + bugs de UI (`windows/phase-2-correctness.md`)
- [ ] Fase 3 Windows — seguridad: pinning real de certificado + CSP + escaping (`windows/phase-3-security.md`). **Sumar acá:** la entrada de autostart (`HKCU\...\Run`) que escribe `tauri-plugin-autostart` no lleva comillas → *unquoted path* (CWE-428) con rutas con espacios. Hoy funciona por la heurística de Windows, pero conviene reescribirla con comillas.
- [ ] **DECIDIR (antes de tocar Android):** núcleo headless vs foreground-only (`android/SPEC.md`)

## 🟡 Cuando se pueda
- [ ] Android Fase A — ciclo de vida + aprobación nativa (`android/phase-A-lifecycle-and-approval.md`)
- [ ] Android Fase B — binding WiFi + streaming a MediaStore (`android/phase-B-discovery-and-storage.md`)
- [ ] Android Fase C — portapapeles, QR, UI móvil (`android/phase-C-clipboard-qr-mobile.md`)

## 🟢 Ideas / algún día
- [ ] Suite de tests real (hoy no hay). Que cada fase que lo pida agregue tests unitarios Rust.
