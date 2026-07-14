> ✅ **IMPLEMENTADA 2026-07-13** — todas las Tareas (2.1, 2.2.a–f, 2.3, 2.4) aplicadas + review adversarial (3 confirmados + 1 endurecimiento). Verificación por máquina + round-trip de datos reales: OK. Verificación FÍSICA (datos reales + UI): PENDIENTE del usuario. Ver `docs/CHANGELOG.md` y `docs/SESSION_HANDOFF.md`. Archivado.

# Millennium Clipboard — Windows · Fase 2: Correctness y seguridad de datos

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Windows (+ Rust compartido, sin tocar Android) · **Prerrequisitos:** ninguno (independiente de la Fase 1) · **Esfuerzo:** ~1.5–2 días · **Riesgo:** med

## Objetivo
Eliminar tres clases de bugs de corrección: (1) persistencia JSON no atómica que puede truncar/resetear silenciosamente los datos del usuario (favorites, settings, aliases, iconos, manual peers, clipboard-sync); (2) estado de UI que se pisa entre sí (selección de peer que se re-apunta sola, `setStatus` clobbereado por el mensaje de grilla, texto entrante destruido por toasts de ACK, barra de progreso compartida entre TX y RX, rename inline destruido por el re-render); (3) el zombie-killer de Windows que apunta al nombre de proceso equivocado y corre incluso en doble-launch de desarrollo, más el swap de update que puede fallar en silencio.

## Definición de "hecho"
- [ ] Existe un módulo `json_store.rs` con un `JsonStore<T>` genérico y los seis stores (`preferences`, `settings`, `aliases`, `icon_overrides`, `manual_peers`, `clipboard_sync`) escriben vía ese store.
- [ ] Toda escritura de JSON va a `<file>.tmp` y hace `rename` atómico sobre el destino; nunca hay un `fs::write` directo sobre el archivo final.
- [ ] Al fallar el parseo, se loguea un `ERR` y el archivo se copia a `<file>.corrupt` **antes** de caer a defaults; nunca más un `unwrap_or_default()` silencioso.
- [ ] En el frontend, cuando el peer seleccionado desaparece del snapshot, el botón TRANSMIT queda deshabilitado y el target muestra `TARGET LOST`; la auto-selección del primer peer ocurre **solo** en la carga inicial.
- [ ] `setStatus` respeta prioridad/TTL: un error no es pisado por el mensaje de grilla (~5 s) hasta que expire su TTL.
- [ ] El texto entrante vive en su propia UI/historial y no lo destruye un toast de `TRANSMIT OK`/ACK, ni viceversa.
- [ ] La barra de progreso del receptor está separada de la del emisor y se keyea por `sessionId`; un RX concurrente no corrompe el bloque de TX.
- [ ] Un rename inline en curso sobrevive a un evento `peers-changed`.
- [ ] `peer.status` se normaliza al ingresar y cada render de peer está envuelto en `try/catch`.
- [ ] El zombie-killer mata el proceso real por dueño del puerto (`Get-NetTCPConnection -LocalPort 53319`) o por ambos nombres de exe, y se saltea cuando `MILLENNIUM_INSTANCE` está seteada.
- [ ] El swap de update reintenta el `move` en un loop y, si falla, escribe un marcador que la app muestra al siguiente arranque.

---

## Tareas

### Tarea 2.1 — Persistencia JSON atómica + `JsonStore<T>` genérico

**Problema.** Los seis stores comparten el mismo patrón copy-paste con dos fallas de corrección:
1. `fs::write(&self.path, payload)` escribe **in-place**. Si el proceso muere (o el zombie-killer lo mata, o hay corte de energía) a mitad de la escritura, el archivo queda truncado/parcial y el próximo arranque lo ve como corrupto.
2. Al cargar, `serde_json::from_str::<T>(&raw).unwrap_or_default()` **descarta silenciosamente** los datos del usuario ante cualquier error de parseo — un solo byte corrupto resetea todos los favorites/manual peers a vacío sin dejar rastro.

**Archivo(s).**
- `src-tauri/src/preferences.rs:44-50` (load) y `:87-94` (persist)
- `src-tauri/src/settings.rs:49-73` (load) y `:136-143` (persist)
- `src-tauri/src/aliases.rs:34-40` y `:70-77`
- `src-tauri/src/icon_overrides.rs:36-42` y `:80-87`
- `src-tauri/src/manual_peers.rs:44-50` y `:84-91`
- `src-tauri/src/clipboard_sync.rs:40-46` y `:92-99`
- Nuevo: `src-tauri/src/json_store.rs`
- `src-tauri/src/lib.rs:22` (declaración de módulos)

**Estado actual.** El patrón de carga (idéntico en los seis, ejemplo de `preferences.rs`):
```rust
let inner = if path.exists() {
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str::<Preferences>(&raw).unwrap_or_default()
} else {
    Preferences::default()
};
```
El patrón de persistencia (idéntico en los seis):
```rust
fn persist(&self, payload: String) -> Result<()> {
    if let Some(parent) = self.path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&self.path, payload)
        .with_context(|| format!("write {}", self.path.display()))?;
    Ok(())
}
```
`settings.rs` es el único que ya loguea el error de parseo (`:54-63`) pero igual cae a defaults sin respaldar el archivo.

**Cambio.** Crear `src-tauri/src/json_store.rs` con un `JsonStore<T>` genérico que encapsule ruta, mutex, carga con backup-on-corrupt y persistencia atómica. Los seis stores conservan su API pública (`add_favorite`, `set`, `snapshot`, etc.) pero delegan el I/O en el `JsonStore`.

Sketch del módulo nuevo:
```rust
// src-tauri/src/json_store.rs
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct JsonStore<T> {
    path: PathBuf,
    inner: Mutex<T>,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Default + Send,
{
    /// Resuelve el nombre respetando MILLENNIUM_INSTANCE (dev double-launch)
    /// y carga el archivo. Ante parseo fallido: loguea ERR, copia el
    /// contenido crudo a `<file>.corrupt` y cae a T::default() — nunca
    /// descarta datos en silencio.
    pub fn load(data_dir: &Path, base: &str, ext: &str) -> Result<Self> {
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("{base}-{s}.{ext}"),
            _ => format!("{base}.{ext}"),
        };
        let path = data_dir.join(filename);

        let inner = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            match serde_json::from_str::<T>(&raw) {
                Ok(v) => v,
                Err(e) => {
                    let corrupt = path.with_extension(format!("{ext}.corrupt"));
                    let _ = fs::write(&corrupt, &raw);
                    crate::runtime_log::err(format!(
                        "[jsonstore] parse failed for {} ({}). Backed up to {} and reset to default.",
                        path.display(), e, corrupt.display()
                    ));
                    T::default()
                }
            }
        } else {
            T::default()
        };

        Ok(Self { path, inner: Mutex::new(inner) })
    }

    /// Acceso mutable + persistencia atómica en una sola llamada. El
    /// closure muta el estado; devolvemos lo que necesite el caller.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R> {
        let (ret, payload) = {
            let mut guard = self.inner.lock().unwrap();
            let ret = f(&mut guard);
            let payload = serde_json::to_string_pretty(&*guard)
                .context("serialize json store")?;
            (ret, payload)
        };
        self.persist(&payload)?;
        Ok(ret)
    }

    /// Lectura de solo lectura sobre el estado.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let guard = self.inner.lock().unwrap();
        f(&guard)
    }

    /// Escribe a `<file>.tmp` y luego renombra sobre el destino. El
    /// rename es atómico dentro del mismo volumen en Windows (ReplaceFile
    /// semantics de `fs::rename` cuando el destino existe).
    fn persist(&self, payload: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, payload)
            .with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), self.path.display()))?;
        Ok(())
    }
}
```

Luego reescribir cada store para envolver un `JsonStore<InnerType>`. Ejemplo con `preferences.rs` (mantener `FavoritePeer` y la struct `Preferences` tal cual, solo cambiar `PreferencesStore`):
```rust
pub struct PreferencesStore {
    store: crate::json_store::JsonStore<Preferences>,
}

impl PreferencesStore {
    pub fn load_or_new(data_dir: &Path) -> Result<Self> {
        let store = crate::json_store::JsonStore::load(data_dir, "prefs", "json")?;
        let n = store.read(|p| p.favorites.len());
        println!("[prefs] loaded {} favorite(s)", n);
        Ok(Self { store })
    }

    pub fn is_favorite(&self, fingerprint: &str) -> bool {
        self.store.read(|p| p.favorites.contains_key(fingerprint))
    }

    pub fn add_favorite(&self, peer: FavoritePeer) -> Result<()> {
        self.store.update(|p| { p.favorites.insert(peer.fingerprint.clone(), peer); })
    }

    pub fn remove_favorite(&self, fingerprint: &str) -> Result<()> {
        self.store.update(|p| { p.favorites.remove(fingerprint); })
    }

    pub fn favorites_snapshot(&self) -> Vec<FavoritePeer> {
        self.store.read(|p| p.favorites.values().cloned().collect())
    }
}
```
Mapeo de `base`/`ext` para cada store (respetar los nombres de archivo EXACTOS ya en uso, para no huérfanar los datos existentes de los usuarios):
| Store | `base` | `ext` | Inner type |
|---|---|---|---|
| `preferences` | `prefs` | `json` | `Preferences` |
| `settings` | `settings` | `json` | `Settings` |
| `aliases` | `aliases` | `json` | `Aliases` |
| `icon_overrides` | `icon-overrides` | `json` | `IconOverrides` |
| `manual_peers` | `manual-peers` | `json` | `ManualPeers` |
| `clipboard_sync` | `clipboard-sync` | `json` | `ClipSync` |

`settings.rs` es el caso especial: su `Settings` **no** deriva `Default` (tiene `download_dir: PathBuf` sin default sensato). Dos opciones, en orden de preferencia:
- **(A) Preferida:** dar a `JsonStore::load` una variante `load_with_default(data_dir, base, ext, default: T)` que reciba el default explícito, y agregar el bound `T: Serialize + DeserializeOwned + Send` (sin `Default`). Mantener `load()` como thin wrapper que llama `load_with_default(.., T::default())` bajo un bound extra `T: Default`. Así `settings` usa `load_with_default` con el `download_dir` calculado, y los otros cinco siguen usando `load()`.
- (B) Si complica los bounds, dejar `settings.rs` con su propia carga (que ya loguea el error) pero agregarle el backup-a-`.corrupt` y la escritura atómica `tmp`+`rename` a mano, reutilizando solo el `persist` atómico. Menos DRY pero válido.

Registrar el módulo en `lib.rs` junto a los demás `mod` (cerca de `src-tauri/src/lib.rs:22`):
```rust
mod json_store;
```

**Por qué.** El `rename` atómico garantiza que el archivo final siempre esté completo o intacto — nunca un estado parcial. El backup a `.corrupt` convierte una pérdida silenciosa de datos en algo recuperable y diagnosticable. El `JsonStore<T>` colapsa ~90 líneas duplicadas por store en un único punto correcto, así el próximo store nace atómico por construcción.

**Cuidado con.**
- NO cambiar los nombres de archivo (`prefs.json`, `settings.json`, etc.) ni la lógica de `MILLENNIUM_INSTANCE`: cambiarlos huérfana los datos de usuarios existentes.
- `fs::rename` en Windows falla si el destino está abierto por otro handle. Como estos archivos solo los toca este proceso y bajo el `Mutex`, no debería haber contención; pero NO tener el `tmp` con la misma extensión final que dispare watchers. Usar `.tmp` como en el sketch.
- El `.corrupt` usa `with_extension(format!("{ext}.corrupt"))` que produce `prefs.json.corrupt` — verificá que `with_extension` no coma el `.json` (con un `PathBuf` cuyo filename es `prefs.json`, `with_extension("json.corrupt")` da `prefs.json.corrupt`; OK). Si dudás, construir el path a mano con `path.as_os_str()` + `".corrupt"`.
- `settings.rs` tiene logging `eprintln!` de diagnóstico muy verboso (`[settings::load] ...`) — se puede conservar o migrar a `runtime_log`, pero NO romper el mensaje final `[settings] download_dir=... auto_accept_favorites=...` del que dependen los diagnósticos.
- `clipboard_sync.rs` tiene además `last_synced_hash: Mutex<Option<String>>` y las funciones libres `hash_text`/`hash_bytes` que NO son parte del JSON persistido — dejarlas fuera del `JsonStore`, viven en el `ClipboardSyncStore` como campo aparte.
- Mantener intactas las firmas públicas que consume `lib.rs`/`discovery.rs`/`http_server.rs` (`is_favorite`, `favorites_snapshot`, `snapshot`, `contains`, `add`, `remove`, `get`, `set`, `clear`, `is_enabled`, `enabled_snapshot`, `snapshot` de settings, etc.).

---

### Tarea 2.2 — Frontend: estado de UI que se pisa entre sí

Seis sub-bugs de corrección en `src/main.js`. Van en orden de dependencia; 2.2.f (normalizar status + try/catch) conviene primero porque endurece el render que las demás tocan.

#### 2.2.a — Normalizar `peer.status` en ingestión y proteger cada render con try/catch

**Problema.** `updatePeerItem` (`:515`) y `buildPeerItem` (`:567`) hacen `p.status.toUpperCase()`. Si un peer llega sin `status` (payload malformado, futura fuente de peers, o un `status` no-string) esto tira `TypeError` y aborta el render **completo** de la lista (el `forEach` de `renderPeers` no está protegido), dejando la grilla congelada. Además `updatePeerItem` hace `statusEl.classList.remove('online', 'offline', 'reaching')` — `'reaching'` no existe: el backend solo emite `"online"` u `"offline"` (`discovery.rs:72,140,161`). Y hay CSS/estado stale de `'away'` que ninguna ruta produce.

**Archivo(s).** `src/main.js:1979` (ingestión en `applyPeers`), `:483-516` (`updatePeerItem`), `:518-570` (`buildPeerItem`), `:458-469` (loop de `renderPeers`).

**Estado actual.**
```js
function applyPeers(wirePeers, initial) {
    state.peers = wirePeers.map((p) => ({ ...p }));
```
```js
const statusLabel = li.querySelector('.status-label');
if (statusLabel) statusLabel.textContent = p.status.toUpperCase();
```
```js
filtered.forEach((p, idx) => {
  seen.add(p.id);
  let li = existing.get(p.id);
  if (li) {
    updatePeerItem(li, p);
  } else {
    li = buildPeerItem(p);
  }
  if (peerList.children[idx] !== li) {
    peerList.insertBefore(li, peerList.children[idx] || null);
  }
});
```

**Cambio.**
1. Normalizar en la ingestión, en `applyPeers`:
```js
function applyPeers(wirePeers, initial) {
    const VALID_STATUS = new Set(['online', 'offline']);
    state.peers = wirePeers.map((p) => {
      const status = VALID_STATUS.has(p.status) ? p.status : 'offline';
      return { ...p, status };
    });
```
2. Reemplazar los dos `p.status.toUpperCase()` (en `updatePeerItem` y `buildPeerItem`) por una lectura segura ya que `status` está normalizado — pero por defensa en profundidad usar `String(p.status || 'offline').toUpperCase()`.
3. En `updatePeerItem`, corregir la lista de clases removidas: `statusEl.classList.remove('online', 'offline')` (sacar `'reaching'`, agregar `'away'` a la lista de removidas si el CSS aún define un estilo `.away` para limpiarlo). No agregar clases que el backend no emite.
4. Envolver el cuerpo del `forEach` de `renderPeers` en try/catch para que un peer roto no tumbe toda la lista:
```js
filtered.forEach((p, idx) => {
  seen.add(p.id);
  try {
    let li = existing.get(p.id);
    if (li) { updatePeerItem(li, p); }
    else { li = buildPeerItem(p); }
    if (peerList.children[idx] !== li) {
      peerList.insertBefore(li, peerList.children[idx] || null);
    }
  } catch (err) {
    console.error('renderPeers: peer render failed', p && p.id, err);
  }
});
```

**Por qué.** Un solo peer con forma inesperada no debe congelar la grilla entera. Normalizar en un punto (ingestión) evita repetir defensas en cada consumidor y elimina las clases/estados stale (`reaching`, `away`).

**Cuidado con.** No filtrar peers con status desconocido (eso los desaparecería); mapearlos a `'offline'` es correcto. El try/catch usa `insertBefore` que en el catch se saltea — está bien: la próxima snapshot reintenta.

#### 2.2.b — No re-apuntar el peer seleccionado en silencio; `TARGET LOST` + TRANSMIT deshabilitado

**Problema.** `applyPeers` auto-selecciona `state.peers[0]` cada vez que `selectedPeerId` es `null` (`:1993-1996`), incluyendo después de que el peer seleccionado desaparece (`:1982-1987` lo pone en `null`). Resultado: el usuario apunta a A, A se cae, y el próximo snapshot re-apunta silenciosamente a B — el usuario puede terminar transmitiendo al peer equivocado. La auto-selección debe ocurrir SOLO en la carga inicial.

**Archivo(s).** `src/main.js:1978-2005` (`applyPeers`).

**Estado actual.**
```js
// Drop selection if the selected peer vanished.
if (state.selectedPeerId && !state.peers.find((p) => p.id === state.selectedPeerId)) {
    state.selectedPeerId = null;
    setText(targetName, '—');
    setText(targetHex, '—');
    if (sendBtn) sendBtn.disabled = true;
}

// Render the list ALWAYS so it stays in sync with state.peers.
renderPeers();

// If nothing selected and peers exist, pick the first.
if (!state.selectedPeerId && state.peers.length > 0) {
    state.selectedPeerId = state.peers[0].id;
    selectPeer(state.selectedPeerId);
}
```

**Cambio.** Distinguir "nunca hubo selección" (carga inicial → auto-seleccionar OK) de "el seleccionado se cayó" (→ mostrar `TARGET LOST`, NO re-apuntar):
```js
// El seleccionado desapareció del snapshot: NO re-apuntar solo.
if (state.selectedPeerId && !state.peers.find((p) => p.id === state.selectedPeerId)) {
    state.selectedPeerId = null;
    state.targetLost = true;
    setText(targetName, 'TARGET LOST');
    setText(targetHex, '—');
    if (sendBtn) sendBtn.disabled = true;
    setStatus('TARGET LOST · peer went offline. Pick another.', { priority: 'warn', ttl: 6000 });
}

// Render la lista SIEMPRE.
renderPeers();

// Auto-seleccionar el primer peer SOLO en la carga inicial (nunca en
// snapshots posteriores) y nunca si acabamos de perder el target.
if (initial && !state.selectedPeerId && !state.targetLost && state.peers.length > 0) {
    state.selectedPeerId = state.peers[0].id;
    selectPeer(state.selectedPeerId);
}
```
Agregar `targetLost: false` a la struct `state` (`:184-194`). En `selectPeer` (`:572-587`), al seleccionar manualmente, limpiar el flag: `state.targetLost = false;`. En `transmit` (`:810-828`), la guarda `if (!state.selectedPeerId)` ya cubre el caso — pero ajustar el mensaje a `'ERR · TARGET LOST — select a peer.'` cuando `state.targetLost`.

**Por qué.** Transmitir al peer equivocado por un re-apuntado silencioso es una falla de datos seria (podés mandar un secreto al peer que no era). El usuario debe re-seleccionar conscientemente.

**Cuidado con.** El mensaje "PEER OFFLINE · waiting on grid" de `selectPeer` (`:580-582`) es para un favorite offline que el usuario eligió a propósito — NO confundir con `TARGET LOST`. `TARGET LOST` es solo cuando el peer **desaparece del snapshot** estando seleccionado.

#### 2.2.c — `setStatus` con prioridad/TTL para que los errores no sean pisados

**Problema.** `setStatus(msg)` (`:212`) es un `setText` crudo. `applyPeers` llama `setStatus('GRID · N peer(s) online.')` en CADA snapshot `peers-changed` (cada ~5 s), pisando cualquier mensaje de error (`ERR transmit`, `ERR rename`, `TARGET LOST`) que el usuario necesita leer.

**Archivo(s).** `src/main.js:212` (`setStatus`), y todos sus callers (los errores deben pasar prioridad).

**Estado actual.**
```js
function setStatus(msg) { setText(statusMsg, msg); }
```

**Cambio.** Dar a `setStatus` un segundo parámetro `opts = { priority, ttl }`. Un mensaje de baja prioridad (info, como el de grilla) no pisa un mensaje de alta prioridad (warn/err) vigente hasta que expire su TTL:
```js
let statusPriorityUntil = 0;   // timestamp hasta el cual hay msg prioritario
const STATUS_LEVEL = { info: 0, warn: 1, err: 2 };

function setStatus(msg, opts) {
  const level = STATUS_LEVEL[(opts && opts.priority) || 'info'];
  const now = Date.now();
  // Un mensaje info no pisa uno prioritario aún vigente.
  if (level === 0 && now < statusPriorityUntil) return;
  setText(statusMsg, msg);
  if (level > 0) {
    const ttl = (opts && opts.ttl) || 5000;
    statusPriorityUntil = now + ttl;
  } else {
    statusPriorityUntil = 0;
  }
}
```
Luego, en los callers de error, pasar prioridad. Como mínimo:
- `:613` `setStatus(\`ERR favorite · ${err}\`, { priority: 'err' })`
- `:629` `setStatus(\`ERR clipboard sync · ${err}\`, { priority: 'err' })`
- `:673` `setStatus(\`ERR rename · ${err}\`, { priority: 'err' })`
- `:871` `setStatus(\`ERR transmit · ${err}\`, { priority: 'err' })`
- `:922` `setStatus(\`ERR transmit · ${err}\`, { priority: 'err' })`
- El `TARGET LOST` de 2.2.b (ya lo pasa como `warn`).

Los `setStatus` de grilla en `applyPeers` (`:1999,2001,2003`) quedan como info (sin opts) y por lo tanto respetan el TTL.

**Por qué.** El status es el único canal donde el usuario ve por qué falló una acción; que se borre en 5 s por un mensaje rutinario lo hace inútil justo cuando importa.

**Cuidado con.** No poner TTL a los mensajes `info` de progreso legítimos que SÍ deben actualizarse rápido (p.ej. `RX · N received`). Esos son info y se auto-pisan entre sí — correcto. Solo `warn`/`err` bloquean.

#### 2.2.d — Separar el texto entrante de los toasts de ACK (historial propio)

**Problema.** `showToast(text)` (`:213-220`) y `showIncomingText(...)` (`:229-295`) escriben en el **mismo** `toastText`/`toast`. Un texto recibido de un peer se muestra en `showIncomingText`, pero cualquier ACK de envío posterior (`showToast('... · ACK')` en `:863,914`) hace `toastText.innerHTML = ''` y destruye el texto entrante antes de que el usuario lo copie. Y al revés: un ACK puede quedar tapado por texto entrante. Además solo se ve el ÚLTIMO entrante; no hay historial.

**Archivo(s).** `src/main.js:213-295` (`showToast`, `showIncomingText`), listener `incoming-text` `:1849-1856`.

**Estado actual.**
```js
function showToast(text) {
    toastText.innerHTML = '';
    toastText.textContent = text;
    setToastTitle('TRANSMIT OK');
    toast.hidden = false;
    if (toastHideTimer) clearTimeout(toastHideTimer);
    toastHideTimer = setTimeout(() => (toast.hidden = true), 3800);
}
```
`showIncomingText` construye botones COPY / SAVE SENDER / CLOSE dentro de ese mismo `toastText` y hace `toast.hidden = false` sin auto-hide.

**Cambio.** Separar las dos superficies. La opción de menor cambio y máximo valor:
1. Dejar `showToast` (ACK efímero) tal cual, pero que NUNCA comparta nodo con el texto entrante.
2. Mover el texto entrante a su propia UI persistente: un panel/lista "INBOX" que acumula los últimos N (p.ej. 20) mensajes recibidos, cada uno con su COPY / SAVE SENDER. Agregar `state.incomingHistory = []` (a `state`, `:184-194`).
3. En el listener `incoming-text` (`:1849`), en vez de reemplazar, hacer `pushIncoming({ text, alias: senderAlias, fingerprint: senderFingerprint, ip: senderIp, port: senderPort, at: Date.now() })` que hace `state.incomingHistory.unshift(entry)`, trunca a 20 y re-renderiza el panel INBOX (cada entrada con los mismos botones que hoy arma `showIncomingText`).

Si un panel INBOX nuevo es demasiado alcance para esta fase, el mínimo viable es: usar **dos contenedores DOM distintos** — `toast` para ACK y un `incomingToast` separado para entrantes — de modo que un ACK jamás llame `innerHTML=''` sobre el nodo del entrante. Reutilizar el markup de botones actual de `showIncomingText` sobre `incomingToast`. El historial en memoria (`state.incomingHistory`) puede sumarse igual para no perder mensajes.

**Por qué.** Recibir un texto y perderlo porque justo mandaste algo (ACK) es pérdida de datos de cara al usuario. Son dos flujos independientes y no deben compartir un nodo destructivo.

**Cuidado con.** `showIncomingText` distingue `isKnownPeer(fingerprint)` para mostrar `+ SAVE SENDER` (`:264-283`) — preservar esa lógica en el nuevo render. El `blip`/`notify` del listener (`:1852-1855`) quedan igual. No romper el botón CLOSE ni el auto-hide del ACK (3800 ms).

#### 2.2.e — Separar el progreso del receptor del bloque del emisor (keyear por `sessionId`)

**Problema.** Los listeners `transfer-progress-sender` (`:1886-1893`) y `transfer-progress-receiver` (`:1895-1904`) escriben en la MISMA barra global (`setProgress`, `progressText`, `progressBlock`). Si estás enviando a A mientras recibís de B, ambos flujos pelean por la misma barra: el `%` salta entre TX y RX y `progressBlock.hidden` se togglea de forma incoherente. El emisor además mantiene `state.activeTransfer.sessionId` (`:892-903`) pero el receptor no keyea por sesión.

**Archivo(s).** `src/main.js:1886-1937` (listeners sender/receiver/session-completed/session-cancelled), barra en `:196-209` y `:884-926`.

**Estado actual.**
```js
await listen('transfer-progress-receiver', (event) => {
  const { bytesReceived, total } = event.payload;
  if (total > 0) {
    const pct = Math.min(99, Math.round((bytesReceived / total) * 100));
    setProgress(pct);
    progressBlock.hidden = false;
    progressText.textContent = `RECEIVING // ${formatBytes(bytesReceived)} / ${formatBytes(total)}`;
    setStatus(`RX · ${formatBytes(bytesReceived)} received`);
  }
});
```

**Cambio.** Darle al receptor su propia superficie de progreso separada del bloque de TX. Estructura:
1. Agregar un segundo bloque de progreso en el DOM (p.ej. `rxProgressBlock` / `rxProgressText` con sus propios segmentos), o —mínimo— un juego de variables de estado y una función `setRxProgress(pct)` independiente de `setProgress`. La barra existente (`progressBlock`, `setProgress`) queda dedicada al EMISOR.
2. Keyear el RX por `sessionId`. Los eventos del backend de RX deberían traer un identificador de sesión; si `event.payload` incluye `sessionId`, guardar `state.activeReceive = { sessionId, ... }` y solo actualizar si coincide:
```js
await listen('transfer-progress-receiver', (event) => {
  const { bytesReceived, total, sessionId } = event.payload;
  if (!total || total <= 0) return;
  if (!state.activeReceive || state.activeReceive.sessionId !== sessionId) {
    state.activeReceive = { sessionId };
  }
  const pct = Math.min(99, Math.round((bytesReceived / total) * 100));
  setRxProgress(pct);
  rxProgressBlock.hidden = false;
  rxProgressText.textContent = `RECEIVING // ${formatBytes(bytesReceived)} / ${formatBytes(total)}`;
  setStatus(`RX · ${formatBytes(bytesReceived)} received`, { priority: 'info', ttl: 0 });
});
```
3. `session-completed` (`:1913`) y `session-cancelled` (`:1933`) deben ocultar el bloque **de RX** (`rxProgressBlock`), NO el de TX. `session-completed` hoy hace `progressBlock.hidden = true; setProgress(0);` (`:1915-1916`) — eso oculta el bloque del emisor por error; cambiarlo a `rxProgressBlock.hidden = true; setRxProgress(0);`. Limpiar `state.activeReceive = null`.

**Nota de dependencia con backend.** Verificar en `http_server.rs`/`transfer.rs` que el evento `transfer-progress-receiver` incluya un `sessionId`; si no lo emite, agregarlo del lado Rust (es un campo más en el payload del `emit`). Si el backend NO tiene el `sessionId` a mano en RX, el mínimo aceptable es solo separar las superficies DOM (paso 1) sin el keyeo, que ya elimina el conflicto TX/RX visual.

**Por qué.** TX y RX son concurrentes por diseño (LAN full-duplex). Compartir una barra hace que el progreso mostrado sea directamente incorrecto.

**Cuidado con.** No romper el flujo de éxito del EMISOR (`transmitFiles` `:898-918`) que pone `setProgress(100)` y luego `progressBlock.hidden = true`. Ese sigue usando `progressBlock`/`setProgress`. Solo el RX se muda a `rxProgressBlock`.

#### 2.2.f — Proteger un rename inline en curso del re-render por `peers-changed`

**Problema.** `startInlineRename` (`:646-683`) reemplaza el `.peer-name` por un `<input>`. Pero un evento `peers-changed` dispara `applyPeers → renderPeers → updatePeerItem`, y `updatePeerItem` hace `setText(li.querySelector('.peer-name'), p.name)` (`:488`), que destruye el `<input>` a mitad de tipeo — el usuario pierde lo que estaba escribiendo.

**Archivo(s).** `src/main.js:646-683` (`startInlineRename`), `:483-516` (`updatePeerItem`).

**Estado actual.**
```js
function startInlineRename(item) {
    const nameEl = item.querySelector('.peer-name');
    if (!nameEl || nameEl.querySelector('input')) return;
    const id = item.dataset.id;
    const original = nameEl.textContent;
    nameEl.innerHTML = '';
    const input = document.createElement('input');
    ...
}
```
y en `updatePeerItem`:
```js
setText(li.querySelector('.peer-name'), p.name);
```

**Cambio.** Marcar el `<li>` como "en rename" y hacer que `updatePeerItem` salte la actualización del nombre (y solo del nombre) mientras dure:
```js
function startInlineRename(item) {
    const nameEl = item.querySelector('.peer-name');
    if (!nameEl || nameEl.querySelector('input')) return;
    item.dataset.renaming = 'true';
    ...
    const finish = async (commit) => {
      if (done) return;
      done = true;
      delete item.dataset.renaming;      // <- limpiar SIEMPRE al terminar
      const newName = input.value.trim();
      ...
    };
}
```
En `updatePeerItem`, guardar el nombre mientras se renombra:
```js
if (li.dataset.renaming !== 'true') {
  setText(li.querySelector('.peer-name'), p.name);
}
```
El resto de `updatePeerItem` (status, ip, hex, iconos, toggles) puede seguir actualizándose sin problema.

**Por qué.** Un `peers-changed` cada ~5 s casi garantiza destruir cualquier rename que tome más de unos segundos. Preservar solo el nombre durante el rename mantiene el resto de la fila viva.

**Cuidado con.** Asegurar que `delete item.dataset.renaming` corra en TODAS las salidas de `finish` (Enter, Escape, blur, y el catch del `invoke('rename_peer')`). El `finish` actual usa un guard `done` — poner el `delete` justo después de setear `done = true` cubre las tres rutas. Si el `<li>` se re-crea (buildPeerItem) durante el rename porque el peer salió y volvió, el input se pierde igual — es un caso raro y aceptable; no intentar resucitarlo.

---

### Tarea 2.3 — Zombie-killer: apuntar al proceso real y saltear en dev

**Problema.** `kill_other_millennium_processes` corre `Get-Process millennium-clipboard`. Pero el nombre del proceso de release es **`Millennium Clipboard`** (con espacio) porque `tauri.conf.json` tiene `"productName": "Millennium Clipboard"`, mientras que `Get-Process millennium-clipboard` solo matchea el binario de `cargo` (dev). Es decir: en release **no mata al zombie real** (el que tiene el puerto 53319), que era justo el propósito. Peor: se llama incondicionalmente (`lib.rs:995`), así que en un doble-launch de desarrollo (`MILLENNIUM_INSTANCE` seteada) una instancia mata a la otra.

**Archivo(s).** `src-tauri/src/windows_integration.rs:97-130` (`kill_other_millennium_processes`), `src-tauri/src/lib.rs:994-995` (call site).

**Estado actual.**
```rust
pub fn kill_other_millennium_processes() {
    let our_pid = std::process::id();
    let ps_cmd = format!(
        "$ErrorActionPreference='SilentlyContinue'; Get-Process millennium-clipboard | Where-Object {{ $_.Id -ne {} }} | ForEach-Object {{ Stop-Process -Id $_.Id -Force; Write-Output $_.Id }}",
        our_pid
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_cmd])
        .output()
    ...
```
Call site:
```rust
#[cfg(target_os = "windows")]
windows_integration::kill_other_millennium_processes();
```

**Cambio.**
1. **Saltear en dev.** Al inicio de la función, salir temprano si `MILLENNIUM_INSTANCE` está seteada (dev double-launch coordina puertos por env var y NO debe matar a su gemelo):
```rust
pub fn kill_other_millennium_processes() {
    if std::env::var("MILLENNIUM_INSTANCE").ok().filter(|s| !s.is_empty()).is_some() {
        crate::runtime_log::info("[win] MILLENNIUM_INSTANCE set — skipping zombie cleanup (dev double-launch)");
        return;
    }
    let our_pid = std::process::id();
```
2. **Matar por dueño del puerto (preferido) y por ambos nombres de exe (fallback).** Preferir identificar el proceso que realmente tiene el puerto 53319 vía `Get-NetTCPConnection -LocalPort 53319`, y como red de seguridad matar por ambos nombres de proceso (`Millennium Clipboard` y `millennium-clipboard`), siempre excluyendo nuestro PID:
```rust
    let port = 53319u16; // discovery::DEFAULT_PORT
    let ps_cmd = format!(
        r#"$ErrorActionPreference='SilentlyContinue';
$our={pid};
$targets=@();
# 1) dueño(s) del puerto de la app
Get-NetTCPConnection -LocalPort {port} -State Listen | ForEach-Object {{ $targets += $_.OwningProcess }};
# 2) por ambos nombres de proceso (release='Millennium Clipboard', dev='millennium-clipboard')
Get-Process -Name 'Millennium Clipboard','millennium-clipboard' | ForEach-Object {{ $targets += $_.Id }};
$targets | Sort-Object -Unique | Where-Object {{ $_ -and $_ -ne $our }} | ForEach-Object {{ Stop-Process -Id $_ -Force; Write-Output $_ }}"#,
        pid = our_pid, port = port
    );
```
El resto de la función (parseo de PIDs de stdout, log, `sleep(400ms)`) queda igual — sigue produciendo la lista de PIDs matados por las líneas de `Write-Output`.

**Por qué.** El bug hace que el killer sea inútil en el binario que los usuarios corren (release), que es exactamente donde el zombie con el puerto colgado aparece. Matar por dueño del puerto ataca la causa raíz ("otro proceso tiene 53319"); los nombres de exe son el fallback por si el zombie ya soltó el puerto pero sigue vivo.

**Cuidado con.**
- `Get-NetTCPConnection` existe en Windows 8+/Server 2012+ (módulo `NetTCPIP`), disponible por defecto en Windows 10/11 — el target de este proyecto. Si por alguna razón no está, el `$ErrorActionPreference='SilentlyContinue'` evita que rompa y el fallback por nombre sigue funcionando.
- El nombre de proceso en `Get-Process` es el nombre del exe SIN extensión, con el espacio: `'Millennium Clipboard'`. Verificá que coincida con `productName` de `tauri.conf.json` (hoy `"Millennium Clipboard"`). Si cambia el `productName`, actualizar acá.
- NO matar por matcheo parcial/wildcard (p.ej. `Millennium*`) para no barrer procesos ajenos del usuario.
- El `-State Listen` en `Get-NetTCPConnection` evita matar conexiones salientes efímeras que casualmente usen ese puerto local; queremos el listener.
- Mantener el `sleep(400ms)` para que Windows libere el TCP port antes de que intentemos bindear.

---

### Tarea 2.4 — Update swap: reintentar el `move` y dejar un marcador de fallo visible

**Problema.** El `.bat` de update (`updater.rs:158-162`) hace un único `move /Y src dst`. Si en ese instante el `.exe` viejo todavía está locked (antivirus escaneando, handle no liberado, el proceso tardó en salir), el `move` falla, el `.bat` igual hace `start ""` del `dst` viejo, se auto-borra (`del "%~f0"`) y el update **se pierde en silencio**: el usuario cree que actualizó pero sigue en la versión vieja, sin ningún error.

**Archivo(s).** `src-tauri/src/updater.rs:157-178` (batch en `download_and_stage`), `src-tauri/src/lib.rs:883-892` (`apply_update`, lado Windows).

**Estado actual.**
```rust
// Tiny self-deleting batch: wait → swap → launch new → delete script.
let bat = format!(
    "@echo off\r\nping 127.0.0.1 -n 3 >nul\r\nmove /Y \"{src}\" \"{dst}\" >nul\r\nstart \"\" \"{dst}\"\r\ndel \"%~f0\"\r\n",
    src = staged.display(),
    dst = current_exe.display(),
);
```

**Cambio.** Reescribir el `.bat` para (a) reintentar el `move` en un loop con espera entre intentos, y (b) si tras N intentos sigue fallando, escribir un archivo marcador que la app lea al próximo arranque. Marcador sugerido: `<temp>\millennium-update-failed.txt`.
```rust
let marker = temp_dir.join("millennium-update-failed.txt");
let bat = format!(
    "@echo off\r\n\
     ping 127.0.0.1 -n 3 >nul\r\n\
     set TRIES=0\r\n\
     :retry\r\n\
     move /Y \"{src}\" \"{dst}\" >nul 2>nul\r\n\
     if not errorlevel 1 goto ok\r\n\
     set /a TRIES+=1\r\n\
     if %TRIES% GEQ 10 goto fail\r\n\
     ping 127.0.0.1 -n 2 >nul\r\n\
     goto retry\r\n\
     :fail\r\n\
     echo update swap failed after %TRIES% tries > \"{marker}\"\r\n\
     start \"\" \"{dst}\"\r\n\
     del \"%~f0\"\r\n\
     goto end\r\n\
     :ok\r\n\
     if exist \"{marker}\" del \"{marker}\"\r\n\
     start \"\" \"{dst}\"\r\n\
     del \"%~f0\"\r\n\
     :end\r\n",
    src = staged.display(),
    dst = current_exe.display(),
    marker = marker.display(),
);
```
Del lado de la app, al arrancar (en el `setup` de `lib.rs`, cerca de donde ya corren los cleanups de Windows, p.ej. después de `kill_other_millennium_processes()` en `lib.rs:995` o dentro del setup callback), chequear el marcador y, si existe, emitir un `backend-error` (o un evento propio `update-failed`) que el frontend ya sabe mostrar, y borrarlo:
```rust
#[cfg(target_os = "windows")]
{
    let marker = std::env::temp_dir().join("millennium-update-failed.txt");
    if marker.exists() {
        let detail = std::fs::read_to_string(&marker).unwrap_or_default();
        let _ = std::fs::remove_file(&marker);
        crate::runtime_log::err(format!("[updater] previous update swap failed: {}", detail.trim()));
        let _ = app.handle().emit(
            "backend-error",
            "Update failed: the new version could not replace the running app (file was locked). Please retry the update.".to_string(),
        );
    }
}
```

**Por qué.** Un update que falla en silencio es peor que uno que falla ruidosamente: el usuario queda en una versión vieja creyendo que está al día, y los bugs "ya arreglados" siguen apareciendo. El reintento cubre el caso común (lock transitorio de AV/handle) y el marcador cubre el caso persistente.

**Cuidado con.**
- `errorlevel` en `.bat`: `move` setea `errorlevel` a 0 en éxito. `if not errorlevel 1` es el idiom correcto para "errorlevel < 1" (o sea == 0, éxito). NO usar `if errorlevel 0` (siempre verdadero). Probar el `.bat` a mano antes de confiar.
- El `%TRIES% GEQ 10` con `ping -n 2` (~1 s por intento) da ~10 s de ventana total — suficiente para que el proceso salga y el AV suelte el handle, sin colgar al usuario indefinidamente.
- `apply_update` (`lib.rs:889-890`) espera 400 ms y luego `app.exit(0)`. El `ping -n 3` inicial del `.bat` (~2 s) debería alcanzar para que el proceso salga; el loop de reintento es el seguro adicional.
- El marcador va en `std::env::temp_dir()` — el MISMO dir que usa `download_and_stage` para `staged`/`script`, así ambos lados coinciden. Si preferís que el marcador sobreviva limpiezas de `%TEMP%`, ponelo en `app_data_dir` y leelo desde ahí; pero mantener consistencia entre el `.bat` (que lo escribe) y la app (que lo lee).
- NO tocar la rama Android de `apply_update`/`updater.rs` (`download_and_stage_apk`, MediaStore) — está fuera del alcance de esta fase Windows.

---

## Cómo verificar

**Build.** Compilar el backend según `../00-SHARED-CONTEXT.md` (comando de build de `src-tauri`). Debe compilar sin warnings nuevos. `cargo clippy` no debe reportar nada nuevo en `json_store.rs`.

**2.1 — atomicidad y backup.**
- Unit test en `json_store.rs` (agregar `#[cfg(test)] mod tests`):
  - `load` de un archivo con JSON inválido debe: (a) devolver `T::default()`, (b) crear un `<file>.corrupt` con el contenido original, (c) NO borrar el archivo original. Assert: `corrupt_path.exists()` y `read(corrupt) == raw_invalido`.
  - `update` seguido de una segunda instancia `load` del mismo path debe leer el valor persistido (round-trip).
  - Simular escritura atómica: tras `update`, no debe existir un `<file>.tmp` residual (assert `!tmp.exists()`).
- Manual: correr la app, agregar un favorite, matar el proceso con Task Manager mientras (idealmente) escribe, reabrir — el favorite debe seguir ahí y no debe haber `prefs.json` truncado. Corromper a mano `prefs.json` (poner `{` suelto), reabrir: los favorites caen a default PERO aparece `prefs.json.corrupt` y en el runtime log una línea `ERR [jsonstore] parse failed for ... Backed up to ...`.

**2.2 — UI.**
- `TARGET LOST`: con dos máquinas (o dos instancias dev con `MILLENNIUM_INSTANCE`), seleccionar el peer B, cerrar B. Observar: el target muestra `TARGET LOST`, `sendBtn` deshabilitado, y NO se auto-selecciona otro peer. Reabrir/seleccionar A manualmente limpia el estado.
- `setStatus` TTL: provocar `ERR transmit` (mandar a un peer que se cae mid-transfer) y confirmar que el texto de error permanece ≥5 s sin ser pisado por `GRID · N peer(s) online.`.
- Texto entrante vs ACK: recibir un texto de un peer, INMEDIATAMENTE mandar un texto a otro (dispara ACK). El texto recibido NO debe desaparecer (queda en el INBOX/panel separado).
- Progreso TX/RX: enviar un archivo grande a A mientras recibís uno de B. Las dos barras avanzan independientes; ninguna salta al `%` de la otra.
- Rename: doble-click en el nombre de un peer, empezar a tipear y esperar >5 s (a que llegue un `peers-changed`). El `<input>` y lo tipeado deben sobrevivir.
- Robustez de render: en la consola del webview, forzar `applyPeers([{ id:'x', name:'X', status: undefined }], false)` — la grilla no debe romperse; el peer aparece como `OFFLINE`.

**2.3 — zombie-killer.** En una máquina Windows, dejar un zombie: lanzar la app release, matar su ventana pero dejar el proceso colgado ocupando 53319 (o simular con `Get-NetTCPConnection -LocalPort 53319` para ver el `OwningProcess`). Relanzar la app: el runtime log debe mostrar `[win] killed N stale ... process(es): [PID]` y el nuevo arranque debe bindear 53319 sin caer al fallback de puerto. En dev con `MILLENNIUM_INSTANCE=2` seteada, el log debe mostrar `[win] MILLENNIUM_INSTANCE set — skipping zombie cleanup` y NO matar la otra instancia.

**2.4 — update swap.** Simular el fallo: correr el `.bat` generado a mano con el `dst` locked (abrir el exe destino en otro proceso que lo mantenga abierto), confirmar que tras ~10 intentos escribe `millennium-update-failed.txt`. Luego arrancar la app: debe emitir el `backend-error`/`update-failed` y borrar el marcador. En el camino feliz (dst no locked), el marcador NO debe quedar y la app nueva arranca.

---

## Riesgo y rollback

- **2.1 (JsonStore)** es el cambio de mayor superficie: toca los seis stores. Riesgo: romper una firma pública que consume `lib.rs`/`http_server.rs`/`discovery.rs`, o cambiar sin querer un nombre de archivo y huérfanar datos. Mitigación: mantener las firmas públicas idénticas y los `base`/`ext` de la tabla. Rollback: los stores viejos son independientes entre sí — se puede revertir store por store dejando el `JsonStore` en su lugar solo para los ya migrados. Es seguro shippear 2.1 solo.
- **2.2 (frontend)** son seis cambios independientes en `main.js`; cada sub-tarea (a–f) es aislada y reversible por separado. La de mayor riesgo visual es 2.2.d/2.2.e (nuevas superficies DOM) — si el panel INBOX o el segundo bloque de progreso da problemas, el fallback "dos contenedores DOM" descrito ya elimina el bug de destrucción sin UI nueva. Ninguna toca el backend salvo el `sessionId` opcional de 2.2.e.
- **2.3 (zombie-killer)** solo corre en Windows y solo al arranque; el peor caso es que mate un proceso que no debía — mitigado por el filtro de PID propio, el skip en dev y evitar wildcards. Rollback: restaurar el `ps_cmd` original.
- **2.4 (update swap)** solo afecta el flujo de auto-update en Windows; el camino feliz es idéntico al actual más reintentos. Rollback: restaurar el `.bat` de una sola línea de `move`. El chequeo del marcador al boot es aditivo y no afecta nada si el archivo no existe.
- Todas las tareas son shippables de forma independiente: 2.1 (Rust stores), 2.2 (JS), 2.3 (Rust Windows), 2.4 (Rust Windows) no dependen entre sí. Recomendado mergear en ese orden por tamaño de blast radius decreciente salvo 2.1 que conviene revisar con calma primero.
