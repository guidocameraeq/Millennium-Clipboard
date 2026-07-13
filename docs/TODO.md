# TODO — Millennium Clipboard

> ÚNICA fuente de pendientes del proyecto. Completado → SE BORRA (la historia vive en CHANGELOG y git). Header de 1 línea, sin narrativa de sesión.

2026-07-13 — ver SESSION_HANDOFF.md

## 🔴 Crítico
- [ ] **Verificar físicamente la Fase 1 con 2 dispositivos** (misma Wi-Fi): parpadeo de peers, CPU en reposo, reaper ~15 s, rescan manual, QR tras cambio de red. Pasos en `SESSION_HANDOFF.md`. Es lo ÚNICO que falta para declarar la Fase 1 VERIFICADA (hoy está implementada + verificada por máquina + review adversarial aplicado, archivada en `docs/archive/`).
- [ ] **Fase 2 Windows — correctness: pérdida de datos + bugs de UI** (`windows/phase-2-correctness.md`). **EMPEZAR ACÁ** una vez OK la verificación física de la Fase 1.
- [ ] Fase 3 Windows — seguridad: pinning real de certificado + CSP + escaping (`windows/phase-3-security.md`). **Sumar acá:** la entrada de autostart (`HKCU\...\Run`) que escribe `tauri-plugin-autostart` no lleva comillas → *unquoted path* (CWE-428) con rutas con espacios. Hoy funciona por la heurística de Windows, pero conviene reescribirla con comillas.
- [ ] **DECIDIR (antes de tocar Android):** núcleo headless vs foreground-only (`android/SPEC.md`)

## 🟡 Cuando se pueda
- [ ] Android Fase A — ciclo de vida + aprobación nativa (`android/phase-A-lifecycle-and-approval.md`)
- [ ] Android Fase B — binding WiFi + streaming a MediaStore (`android/phase-B-discovery-and-storage.md`)
- [ ] Android Fase C — portapapeles, QR, UI móvil (`android/phase-C-clipboard-qr-mobile.md`)

## 🟢 Ideas / algún día
- [ ] Suite de tests real (hoy no hay). Que cada fase que lo pida agregue tests unitarios Rust.
