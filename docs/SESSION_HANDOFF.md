# SESSION_HANDOFF — Millennium Clipboard

> Save game del proyecto. `/cierre` lo SOBREESCRIBE ENTERO en cada sesión — acá nunca se apila historia (eso vive en CHANGELOG). El hook SessionStart lo inyecta en cada chat nuevo.

**Cierre**: 2026-07-13 · **Último commit de código**: `c6a9adc`. Los docs de cierre + el archivado van en commits aparte.

## Qué se hizo
- **Fase 1 de Windows (discovery / fin del parpadeo) IMPLEMENTADA** (spec archivado en `docs/archive/phase-1-discovery.md`), un commit por Tarea:
  - **1.6** `MissedTickBehavior::Skip` en los 2 intervalos tokio (udp + poller). (`4533045`)
  - **1.1** Campo `confirmed` en `PeerRecord` + **política única de reconciliación**: mDNS ya no pisa una ruta confirmada; la src IP del datagrama UDP manda siempre y ahora emite `peers-changed` al corregir. Fn pura `reconcile_mdns` + 3 tests. (`e4f4459`)
  - **1.3 (udp)** Borrado `derive_subnet_broadcast` (broadcast dirigido /24) — queda solo el limited broadcast global. (`b8d53ba`)
  - **1.2/1.3/1.4/1.5** El poller único (probe TCP a TODOS cada 6 s) → **dos tasks**: reaper por `last_seen` (TTL 15 s = 3× UDP) + probe scheduler con backoff exponencial que solo sondea a quien UDP no mantiene fresco. Sin browse por tick, sin gate /24, sin contador u8 (backoff/probe_at purgados por tick). (`58d0b64`)
  - **1.7** `compute_local_ip` elige la placa privada no-virtual (`list_afinet_netifas`, **sin dep nueva**) + fn pura `pick_local_ipv4` + 3 tests + watcher de red cada 30 s que re-anuncia mDNS al cambiar de IP. (`9a6625a`)
- **Review adversarial multi-agente** (5 dimensiones × 2 escépticos): 9 hallazgos, 0 refutados, **5 confirmados + 3 nits aplicados** (`c6a9adc`). Lo más jugoso: el `join_all` serializaba el scheduler al timeout de 5 s y podía reapear un peer vivo (volvía el parpadeo) → ahora `FuturesUnordered`; rescan ahora fuerza probe (Notify); el QR reflejaba la IP vieja tras un roam → IP compartida y viva.

## Estado
- Branch `main`. **Build verde por máquina**: `cargo check` OK, `cargo clippy` sin warnings nuevos (13, los mismos pre-existentes), `cargo test` 7/7 (3 reconcile + 3 pick_ip + 1 de Fase 0), `cargo build` linkea, `node --check src/main.js` OK.
- Diff de la fase: 4 archivos, +646 / −275. Toca `discovery.rs`, `udp_discovery.rs`, `identity.rs`, `lib.rs`.
- **NO se hizo `git push`** (esperando OK del usuario).

## En curso
- Nada. Fase 1 implementada y con review aplicado.

## Próximo paso CONCRETO
1. **Verificación física de la Fase 1 con 2 dispositivos en la misma Wi-Fi** (esto es lo único que falta para declararla VERIFICADA — hoy está solo verificada por máquina). Mirar el panel de LOG y la lista de peers:
   - **Parpadeo**: una PC con WSL/Hyper-V/VPN. Antes: `[udp] IP DISAGREEMENT` + `probe failed`/`DROPPED` en loop. Ahora: `[udp] correcting IP for … (datagram src wins)` **una vez** y el peer fijo online.
   - **CPU en reposo** (Task Manager): bajo/nulo; en el log NO deben repetirse líneas `[probe] …` si ambos se ven por UDP.
   - **Reaper**: cerrar de golpe un peer → el otro lo marca offline en ~15 s.
   - **Rescan**: un peer manual caído que se prende → el botón rescan lo trae al toque.
   - **QR**: cambiar de red y abrir el QR → muestra la IP nueva.
2. Si la verificación física da OK → arrancar **Fase 2 de Windows (correctness)** (`docs/remediation/windows/phase-2-correctness.md`) en chat nuevo con `/inicio`.

## Bloqueos
- **Android**: decisión estratégica previa pendiente (núcleo headless vs foreground-only, `docs/remediation/android/SPEC.md`). No arrancar Android sin decidirla.

## Pendiente derivado (no urgente)
- **Autostart sin comillas** (va a la Fase 3 seguridad): la entrada `HKCU\...\Run` que escribe `tauri-plugin-autostart` no lleva comillas (*unquoted path*, CWE-428). Ya anotado en TODO.

## Contexto que no está en otro doc
- **Divergencias con el spec de Fase 1** (la Fase 0 había movido código):
  - El poller de clipboard ya NO es un `tokio::interval` (Fase 0 lo pasó a `std::thread` + `sleep`), así que la Tarea 1.6 aplicó a 2 intervalos, no 3. El `saturating_add` de la Tarea 1.5 ya estaba puesto por Fase 0.
  - Tarea 1.7: se reusó `local_ip_address::list_afinet_netifas()` (crate ya presente) en vez de sumar `if-addrs` → mismo fix, cero dep nueva (mismo criterio que la Fase 0 con `user32`).
  - El watcher de IP re-anuncia mDNS pero NO reinicia el broadcaster UDP: bindea `0.0.0.0` y su src IP sigue al SO; los peers aprenden la IP nueva por el src del datagrama (Tarea 1.1).
- **Nit del review aceptado, NO arreglado**: el probe scheduler clona manual+favoritos+live cada 2 s aunque no haya nada que sondear — costo despreciable (unas Vec chicas), no es bug. Micro-opt futura si alguna vez molesta.
- **Entorno**: PowerShell 5.1 rompe los `git commit -m` con comillas dobles; usar `git commit -F -` con heredoc desde el Bash tool. La app que corre a diario es la copia del escritorio (`OneDrive\Desktop eQ\Millennium Clipboard.exe`, con espacio en la ruta).
