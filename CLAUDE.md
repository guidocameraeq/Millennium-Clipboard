# CLAUDE.md — Reglas para Claude Code en Millennium Clipboard

> Solo reglas de comportamiento. Arquitectura, schemas, debugging → archivos dedicados (ver mapa de docs).
> Test por línea antes de agregar nada: **si borro esta línea, ¿Claude se equivoca? Si no, sobra.**
> Qué NO va acá: estado, versiones, pendientes (rotan y quedan viejos), arquitectura, schemas.

## Qué es esto

**Millennium Clipboard**: utilidad **solo-LAN** (sin nube, sin cuentas) que comparte texto, archivos y portapapeles entre **Windows desktop** y **Android** en la misma Wi-Fi. Stack: **Tauri 2** — backend Rust (~5.5k LOC en `src-tauri/src/`), frontend JS/CSS vanilla sin framework ni bundler (~5k LOC en `src/`); transporte HTTPS (axum + rustls) con certificados auto-firmados por dispositivo. El dueño la usa a diario en desktop pese a los bugs; la de Android nunca funcionó bien. No es programador fuerte de Rust/sistemas.

**Comandos clave** (desde la raíz `millennium-clipboard/`):
- `npm run tauri dev` — desktop con hot-reload del frontend.
- `npm run tauri build` — build release. `bundle.active=false` → **NO** genera instalador; el artefacto es `src-tauri\target\release\millennium-clipboard.exe`.
- `cd src-tauri; cargo check` — chequeo rápido del backend. **Primer paso ante cualquier "no compila / no funciona".**
- `npm run tauri android build --apk` — build Android firmado (requiere `src-tauri/gen/android/keystore.properties`, gitignored).

## El ciclo de trabajo

1. **Chat nuevo por misión.** El hook SessionStart inyecta el handoff automáticamente; `/inicio` arranca el ritual. **No toco nada hasta el OK del usuario.**
2. Trabajar con las reglas de abajo operando solas.
3. `/cierre` al terminar → docs al día, commit, push → **el chat se descarta**. Cambió la misión → cambia el chat. Compactar es emergencia (`cierre parcial`), no forma de vida.

Los rituales viven en `.claude/skills/`: **`/inicio` · `/cierre` · `/smoke`**. Ahí está el detalle; no lo duplico acá.

## 🚨 Restricciones duras

1. **Los datos y taxonomías del usuario son LA fuente de verdad**: favoritos, alias, iconos y ajustes que él creó NUNCA se transforman, colapsan ni renombran sin mostrar antes una tabla **"esto cambio / esto preservo"** y esperar su OK. En migraciones de stores JSON: checkpoint + diff de decisiones obligatorio.
2. **Secretos**: nunca en el chat ni en comandos — viajan por archivo local. Las passwords del keystore Android (`.keystore/` + `keystore.properties`) no se repiten ni se escriben a ningún archivo persistente.
3. **Intocables**:
   - `docs/remediation/` es el **spec del fix**: se **ejecuta**, no se reescribe. Una fase implementada se marca en su línea 1 y se archiva.
   - Los archivos Android en `src-tauri/gen/android/` se editan **a mano**; **NUNCA correr `tauri android init`** (regenera y pisa `MainActivity.kt`, `MillenniumService.kt`, el manifest, `network_security_config.xml`, `build.gradle.kts`).

## Reglas de evidencia (contra los "fixes fantasma")

1. **Un bug se declara resuelto SOLO tras reproducir E2E el caso que lo disparó** y verlo funcionar. "Apliqué el parche" ≠ "está resuelto". Vale doble acá: el consumo de CPU/RAM se verifica en el Task Manager, no de palabra.
2. Todo resultado se reporta con **evidencia real** (logs, capturas, `cargo check` verde) o como **NO VERIFICADO**. Nunca "listo" a secas.
3. **En tareas aprobadas no freno a pedir permiso intermedio.** Trabajos >2 min → `run_in_background` y reporte al terminar; nunca dejar al usuario esperando sin ETA. Servidores/`tauri dev` SIEMPRE en background.
4. **Post-compactación asumo que NO leí ningún archivo**: Read antes de cualquier Edit.
5. Ante errores: diagnosticar root cause, no aplicar parches encima.
6. **Toda spec se escribe en el formato del anexo** `~/.claude/skills/arquitecto/anexos/formato-spec.md`. El fix ya está especificado en `docs/remediation/`; una feature nueva sobre lo que ya anda = **SPEC delta** con su **NO SE TOCA**.

## Reglas operativas del stack

> Las piedras con las que este proyecto ya tropezó (verificadas en el audit 2026-07-06). Instrucción repetida 3+ veces → va acá.

1. **NO reescribir la app.** Verdicto del audit: el núcleo (motor de transferencia con cliente pooled + streaming + resume, identidad cert+fingerprint, disciplina de locks, frontend push-based con diff incremental) es **bueno**. El trabajo es **cirugía dirigida por fases**, en orden, una fase por vez, verificando el criterio de aceptación antes de seguir.
2. **Locks:** nunca sostener un `std::sync::Mutex` a través de un `.await` — clonar el dato en un scope corto y soltar el lock antes. Es de lo mejor del código; no lo rompas.
3. **Trabajo bloqueante** (IO de archivos, decodificar imágenes, `arboard`) va en `tokio::task::spawn_blocking`. No bloquear el reactor.
4. **Logging:** `runtime_log::info/warn/error(...)`, **nunca `println!`**. (La Fase 0 cambia la parte del IPC de `runtime_log`; respetar esa API.)
5. **Compatibilidad de protocolo:** hay peers viejos en la red. **No romper** el formato del hello UDP ni el JSON de `/info`; si extendés, campos **opcionales**.
6. **Frontend:** un solo `state` global; peers por **diff incremental** (`buildPeerItem`/`updatePeerItem`), no `innerHTML` masivo. **Escapá** todo string que venga de un peer antes de meterlo al DOM (`textContent`/`createElement`).
7. **Android:** el Rust de Android está tras `#[cfg(target_os = "android")]` — para verlo compilar de verdad usá `npm run tauri android build`, **no** `cargo check` (compila para el host). Editá `src-tauri/gen/android/...` directo; jamás `tauri android init` (regla dura #3).
8. **Git:** un commit por Tarea/fase, mensaje imperativo. **No `git push` ni cambio de branch salvo que el humano lo pida.**
9. **Mensajes multi-tema del usuario** (dicta por voz): confirmar el orden de prioridad antes de ejecutar si no dijo "primero X".

## Mapa de documentación

| Archivo | Rol | Quién lo escribe / cuándo |
|---|---|---|
| `docs/SESSION_HANDOFF.md` | **Save game** — foto única de dónde quedamos | `/cierre` lo sobreescribe entero; el hook lo inyecta al inicio |
| `docs/TODO.md` | ÚNICA fuente de pendientes | `/cierre`: completadas afuera, nuevas adentro |
| `docs/CHANGELOG.md` | Historia de cambios | `/cierre`: entrada por sesión |
| `docs/remediation/` | **Spec ejecutable del fix** (audit 2026-07-06) | Se ejecuta por fases; implementada → línea 1 + `docs/archive/` |

**Reglas del sistema**: una sola narrativa por sesión (HANDOFF + CHANGELOG y nada más). Un número vive en UN archivo. Lo derivable del código o de git NO se escribe en un doc. SPEC/fase implementada → estado en línea 1 y a `docs/archive/`.

## Comunicación

- Español rioplatense (vos), **en criollo: corto, sin jerga técnica** — el usuario no es programador. Aplica a TODO.
- Decisiones: opciones con pros/contras + UNA recomendación justificada.
- Explicar QUÉ cambié, no narrar el proceso.
