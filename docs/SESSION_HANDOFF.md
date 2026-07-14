# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-14 · **Último commit de código**: `5bb57e4` (bump 0.16.0). **Release v0.16.0 publicado** (final, `--latest`) y **desplegado en las 2 PCs por auto-update**. `main` pusheado a GitHub.

## Qué se hizo

**Fase 3 (Rust compartido + frontend) — SEGURIDAD — IMPLEMENTADA** (spec archivado en `docs/archive/phase-3-security.md`), un commit por Tarea + 2 del review:

- **3.7** `safe_join` rechaza nombres reservados de Windows (`CON/PRN/AUX/NUL/COM1-9/LPT1-9`), ADS (`:`), chars ilegales NTFS y dots/espacios finales; corre para `rel_path` y `name`. No rompe `report.final.pdf`. (`93439f8`)
- **3.5** `DefaultBodyLimit` **por-ruta**: `/clipboard/image` 48 MiB, `/prepare-upload` 8 MiB; resto 2 MiB; `/upload` sin límite (streamea). (`3357f98`)
- **3.6** Updater verifica SHA-256 del binario **antes** de stagear; aborta si no matchea o no hay hash. (`7108200`) + fix del review: lee el hash del `digest` **per-asset** de GitHub, no del body (`39df5a9`).
- **3.4** `/text` documentado como **abierto** (decisión del dueño — solo dispara toast, no toca portapapeles ni disco). (`ea70950`)
- **3.3** Sacar datos de peer de `innerHTML`: thumbnail (createElement + validar `data:image/`), `senderIp`/`senderPort` (textContent), QR-error (textContent), y `iconType` de mDNS (por `dataset`, no interpolado). (`cd846d0`)
- **3.2** CSP estricta + 4 fuentes auto-hospedadas (`src/fonts/*.woff2`) + `src/pre.js` + 3 `onclick` migrados a `addEventListener`. (`58a900a`)
- **3.1** Cert pinning real (`PinnedFingerprintVerifier`): saca `danger_accept_invalid_certs`, `client_for` cachea Client por-fingerprint (pooling vivo), elimina el probe `/info` spoofeable. (`37502ad`) + fix del review: el verifier **valida la firma del handshake** (`ea65de1`).

**Review adversarial** (5 dim × 2 escépticos): 1 CRÍTICO confirmado (el verifier no validaba la firma → MITM con cert copiado) + 1 defecto del updater (hash no atado al asset). Ambos arreglados. 2 refutados (bypass Unicode COM¹/²/³ de `safe_join`, probado en la máquina real que no redirige con prefijo de dir; framing "updater 100% roto").

## Estado

- **Branch**: `main` (pusheado). **Working tree**: limpio salvo los docs de este ajuste.
- **Build (por máquina): OK** — `cargo check`/`clippy` (9-10 warnings, todas pre-existentes + 1 `#[allow]`; 0 nuevas reales) / `cargo build` release linkea (`.exe` 9.8 MB) / `node --check` main.js+pre.js OK.
- **4 harness aislados verdes** (scratchpad, `rustc`/`cargo` sin Tauri): `safe_join`, `extract_sha256`, verifier de pinning, y **handshake TLS real e2e** (peer real→OK; cert copiado+clave distinta→FAIL `BadSignature`; TOFU→OK).
- **VERIFICADO FÍSICAMENTE por el usuario (2026-07-14)** — el **core de Fase 3 anda en las 2 PCs**: el **auto-update** funcionó (v0.15.0 → v0.16.0 en ambas) y las **transferencias bidireccionales** funcionan → el **cert pinning NO rompió el uso diario** y la **CSP NO rompió la app**. Falta solo lo opcional (ver TODO 🟢): el ataque simulado con otro cert (ya probado por máquina con el harness → `BadSignature`), el bulk de ~50 archivos, y F12-limpio explícito. **No bloquea nada.**

## Próximo paso CONCRETO

Fase 3 cerrada y desplegada. El siguiente pendiente real del proyecto es la **verificación física de la Fase 2** por el usuario (datos que sobreviven a un cierre abrupto + bugs de UI; detalle en `docs/TODO.md` 🔴). Después: decidir el rumbo de Android (`android/SPEC.md`, headless vs foreground-only) antes de tocar ese código.

## Bloqueos

- Ninguno. Fase 3 verificada en su core y en producción (v0.16.0 en las 2 PCs). Queda pendiente la verificación física de Fase 2 (no bloquea seguir).

## Archivos tocados (Fase 3)

- **Rust**: `http_client.rs` (verifier + pinning + firma), `lib.rs` (call-sites + crypto provider + poller clipboard + apply_update), `discovery.rs` (poller pin-eado), `http_server.rs` (safe_join + body limits + doc `/text`), `updater.rs` (SHA-256 + digest), `Cargo.toml` (comentario rustls).
- **Frontend**: `main.js` (escaping + iconSvg + apply_update + add-peer-cancel), `index.html` (CSP: sin Google Fonts, `pre.js`, sin onclick), `styles.css` (`@font-face`), `pre.js` (nuevo), `src/fonts/*.woff2` (4, nuevos).
- **Config**: `tauri.conf.json` (CSP).

## Contexto importante

- **Divergencia con el spec (anotada)**: la Tarea 3.6 del spec elegía el hash del **body** del release; se cambió a leer el `digest` per-asset de GitHub (el body no lo trae en ningún release real → habría abortado siempre; y el body no ata el hash a la plataforma → rompía Android). El fail-safe (abortar sin hash) y la decisión del dueño se mantienen.
- **El código del spec tenía un bug crítico**: el `PinnedFingerprintVerifier` de ejemplo devolvía `Ok(assertion())` en las 2 funciones de firma → el review lo cazó. El spec no siempre es correcto al pie (ver memoria `rustls-pinning-verifier-firma`).
- **Limitación de fondo anotada en TODO** (🟡): sin mTLS, el server no autentica al cliente → cualquier gate por `sender_fingerprint` es spoofeable por quien conozca una huella conocida. Cerrarlo es grande.
- **Android**: los fixes compartidos (`http_client.rs` 3.1, frontend 3.3) también lo protegen, pero está tras `#[cfg(target_os="android")]` → verificado por máquina en el host, NO en dispositivo. NUNCA correr `tauri android init`.
- Tests nuevos van `#[cfg(all(test, not(windows)))]` por el bug de carga del binario de test en Windows (comctl32-v6); se verifican en harness aislado.
