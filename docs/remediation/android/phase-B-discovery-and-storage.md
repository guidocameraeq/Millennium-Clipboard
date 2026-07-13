# Millennium Clipboard — Android · Fase B: Descubrimiento en WiFi + almacenamiento por streaming

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Android (con un arreglo compartido que también mejora el caso multi-NIC de Windows) · **Prerrequisitos:** Fase 1 de Windows (`../windows/phase-1-discovery.md`, Tarea 1.3 — remoción del descarte por `/24`) para la Tarea B.2 · **Esfuerzo:** ~1.5–2 días · **Riesgo:** med

## Objetivo
Hacer que Android sea descubrible y descubra a otros peers de forma fiable en la LAN Wi-Fi, resolviendo explícitamente la IP de la interfaz Wi-Fi (`wlan0`/RFC1918) en vez de aceptar lo que devuelva `local_ip_address::local_ip()` (que en un teléfono puede ser la IP celular, una IP de VPN, o vacío). Y eliminar los picos de RAM que hoy cargan archivos enteros en memoria: la recepción de archivos y la escritura del APK de actualización deben hacerse por **streaming** hacia MediaStore, no con un buffer del archivo completo. De paso, endurecer el path de envío por SAF (colisiones de nombre, tamaño `0 B` en la cola, copias de caché huérfanas) y limpiar `network_security_config.xml`.

## Definición de "hecho"
- [ ] En un teléfono con Wi-Fi + datos móviles activos a la vez, `identity.local_ip` resuelve a la IP de `wlan0` (RFC1918), no a la IP celular ni a `""`.
- [ ] Si no hay ninguna interfaz Wi-Fi/LAN válida, el arranque **loguea un error visible** (`[net] no usable Wi-Fi/LAN IPv4 address ...`) en vez de continuar en silencio con `local_ip=""`.
- [ ] Recibir un archivo de 500 MB en Android hace que la RSS del proceso suba unos pocos MB (tamaño de chunk), **no** ~500 MB. Verificable con `adb shell dumpsys meminfo <pkg>` antes/después.
- [ ] El archivo recibido aparece en `Downloads/` vía MediaStore y **no** queda una copia en el sandbox privado.
- [ ] Aplicar una actualización descarga el APK a `Downloads/` sin cargar el APK entero en RAM.
- [ ] Enviar dos archivos con el mismo `displayName` desde el picker SAF en la misma sesión no se pisan entre sí en la caché; la cola muestra el tamaño real (no `0 B`); la copia de caché se borra tras el upload.
- [ ] `network_security_config.xml` ya no contiene entradas `<domain>` con IPs de red (`10.0.0.0`, etc.); el archivo compila y la app sigue pudiendo hablar cleartext con localhost si hiciera falta.
- [ ] `cargo check` (host) y el build de Android (`../00-SHARED-CONTEXT.md`) pasan.

## Tareas

### Tarea B.1 — Resolver la IP de la interfaz Wi-Fi en Android (y elegir mejor NIC en Windows)

**Problema.** `compute_local_ip()` delega en `local_ip_address::local_ip()`, que devuelve la IP de la ruta de menor métrica. En un teléfono con datos móviles esa ruta suele ser la celular (o una VPN), no `wlan0`; y si algo falla devuelve `String` vacío que se propaga en silencio. Con la IP equivocada, mDNS se enlaza a la interfaz equivocada, el broadcast UDP calcula el subnet-broadcast equivocado (`udp_discovery.rs::derive_subnet_broadcast`), y el `register_self` de mDNS anuncia una dirección inalcanzable. El peer nunca aparece.

**Archivo(s).** `src-tauri/src/identity.rs:109-113` (la función a reemplazar) y `src-tauri/src/lib.rs:1124-1125` (donde conviene surfacear el error).

**Estado actual.**
```rust
// identity.rs:109
fn compute_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}
```

Contexto de apoyo — la enumeración de NICs ya existe y se usa en el arranque (`runtime_log.rs:141`):
```rust
pub fn log_network_interfaces() {
    match local_ip_address::list_afinet_netifas() {
        Ok(list) => {
            info(format!("[net] {} IPv4 interface(s) detected:", list.len()));
            for (name, ip) in list {
                info(format!("[net]   {} -> {}", name, ip));
            }
        }
        Err(e) => { err(format!("[net] list_afinet_netifas failed: {}", e)); }
    }
}
```

**Cambio.** Reescribir `compute_local_ip()` para que enumere interfaces con `local_ip_address::list_afinet_netifas()` (API confirmada: `fn list_afinet_netifas() -> Result<Vec<(String, IpAddr)>, Error>`, ya presente en la crate `local-ip-address = "0.6"`) y **elija** una IPv4 privada, con preferencia por la interfaz Wi-Fi/LAN. No inventar plugins JNI: la enumeración de interfaces basta y funciona igual en Android y en Windows.

Algoritmo de scoring (mayor puntaje gana; descartar loopback, link-local `169.254.*`, e IPv6):

1. La interfaz se llama `wlan0` / empieza con `wlan` (Android Wi-Fi) → +100.
2. En desktop, nombres típicos de Wi-Fi/Ethernet física (`en`, `eth`, `wlan`, `Wi-Fi`, `Ethernet`) → +50; nombres de virtuales conocidos (`vEthernet`, `VMware`, `VirtualBox`, `Hyper-V`, `WSL`, `Docker`, `tun`, `tap`, `utun`, `ppp`, `rmnet`, `radio`) → −100 (rmnet/radio es la interfaz celular en Android).
3. La IP está en un rango RFC1918 (`10/8`, `172.16/12`, `192.168/16`) → +30.

Sketch representativo (adaptalo, no lo copies textual sin revisar los nombres de virtuales para el parque real):

```rust
use std::net::{IpAddr, Ipv4Addr};

fn compute_local_ip() -> String {
    match resolve_lan_ipv4() {
        Some(ip) => ip.to_string(),
        None => {
            // Antes esto devolvía "" en silencio. Ahora lo dejamos
            // vacío PERO logueamos fuerte para que el arranque decida.
            crate::runtime_log::err(
                "[net] no usable Wi-Fi/LAN IPv4 address found — discovery will not work".into(),
            );
            String::new()
        }
    }
}

/// Devuelve la mejor IPv4 privada para descubrimiento LAN, o None.
/// Prefiere wlan0/RFC1918 sobre celular/VPN/virtuales.
fn resolve_lan_ipv4() -> Option<Ipv4Addr> {
    let ifaces = local_ip_address::list_afinet_netifas().ok()?;
    let mut best: Option<(i32, Ipv4Addr)> = None;
    for (name, ip) in ifaces {
        let v4 = match ip {
            IpAddr::V4(v4) => v4,
            IpAddr::V6(_) => continue,
        };
        if v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() {
            continue;
        }
        let mut score = 0i32;
        let lname = name.to_ascii_lowercase();
        if lname.starts_with("wlan") { score += 100; }
        else if ["en", "eth", "wi-fi", "wlan", "ethernet"].iter().any(|k| lname.contains(k)) { score += 50; }
        if ["vethernet", "vmware", "virtualbox", "hyper-v", "wsl",
            "docker", "tun", "tap", "utun", "ppp", "rmnet", "radio", "ccmni"]
            .iter().any(|k| lname.contains(k)) { score -= 100; }
        if is_rfc1918(v4) { score += 30; }
        if best.map_or(true, |(bs, _)| score > bs) {
            best = Some((score, v4));
        }
    }
    best.map(|(_, ip)| ip)
}

fn is_rfc1918(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 10
        || (o[0] == 172 && (16..=31).contains(&o[1]))
        || (o[0] == 192 && o[1] == 168)
}
```

Nota: `Ipv4Addr::is_link_local()` y `is_loopback()`/`is_unspecified()` son métodos estables de `std`. No usar `is_global()`/`is_private()` (aún inestables en el toolchain del proyecto — ver `../00-SHARED-CONTEXT.md`); por eso `is_rfc1918` es manual.

Además, tratar el `local_ip` vacío como **error de arranque surfaceado**, no como "seguir en silencio". En `lib.rs:1124` la identidad ya se carga; justo después (donde hoy está el `[diag] identity ...` en `lib.rs:1233`) agregar:

```rust
if identity.local_ip.is_empty() {
    runtime_log::err(
        "[net] identity.local_ip is empty — mDNS/UDP discovery disabled this run".into(),
    );
    // (opcional) emitir un evento al frontend para un banner "sin red LAN".
}
```

Mantené `local_ip` como campo `#[serde(skip)]` calculado por corrida (identity.rs:24-25); no lo persistas.

**Por qué.** El descubrimiento entero (mDNS `enable_interface` en `discovery.rs:240`, `register_self` en `discovery.rs:557-579`, y el subnet-broadcast de UDP en `udp_discovery.rs:203-218`) se alimenta de `identity.local_ip`. Si esa IP es la celular o vacía, nada de eso puede funcionar en Android. Elegir `wlan0`/RFC1918 explícitamente es la raíz del "Android nunca funcionó bien" en LAN.

**Cuidado con.**
- No rompas el caso desktop: en Windows `list_afinet_netifas()` devuelve nombres como `"Wi-Fi"`, `"Ethernet"`, `"vEthernet (WSL)"`. El scoring debe seguir prefiriendo la NIC física real; por eso los virtuales van a −100. Probá en la máquina de dev que el puntaje elige la NIC correcta (mirá el log `[net] ... -> ...` que ya imprime `log_network_interfaces`).
- `local_ip_address::local_ip()` NO debe seguir usándose como única fuente. Podés dejarlo como último recurso (score 0) sólo si `list_afinet_netifas()` falla, pero no como primera opción.
- Un teléfono legítimamente puede no tener Wi-Fi (sólo datos): ahí `local_ip` vacío + log de error es el comportamiento correcto; no fabriques una IP celular.
- No cambies la firma pública de `Identity` ni de `load_or_generate`; sólo el cuerpo de `compute_local_ip` y el chequeo en `lib.rs`.

---

### Tarea B.2 — Remoción del descarte por `/24` (compartida con Windows Fase 1)

**Problema.** El poller de presencia en `discovery.rs:362-383` descarta cualquier peer cuya IP esté en un `/24` distinto al nuestro y además lo **borra** del cache vivo (`peers_for_poll.lock().unwrap().remove(fp)`). En Android esto es doblemente dañino: si `identity.local_ip` estaba mal (ver B.1) el `my_prefix` es basura y se descartan peers válidos; y aun con B.1 arreglado, la lógica `/24` rompe redes `/16` o con VLANs. Este arreglo **no se duplica acá**.

**Archivo(s).** El cambio vive en `../windows/phase-1-discovery.md`, **Tarea 1.3** (remoción del bloque `retain` de `/24` en `src-tauri/src/discovery.rs:362-383` y de la función `subnet_prefix_24` en `discovery.rs:529-540` si queda sin uso).

**Estado actual (sólo para referencia — no lo edites desde este spec).**
```rust
// discovery.rs:366
let my_prefix = subnet_prefix_24(&my_ip_poll);
by_fp.retain(|fp, (ip, _, _, _)| {
    if let (Some(mine), Some(theirs)) = (my_prefix.as_ref(), subnet_prefix_24(ip)) {
        if mine != &theirs {
            // ... loguea "different /24" y remove(fp) del cache vivo ...
            peers_for_poll.lock().unwrap().remove(fp);
            return false;
        }
    }
    true
});
```

**Cambio.** Ninguno en este archivo. **Dependencia:** Android requiere que la Tarea 1.3 de Windows Fase 1 ya esté aplicada (el código de `discovery.rs` es compartido entre plataformas). Si ejecutás las fases de Android antes que la Fase 1 de Windows, aplicá primero la Tarea 1.3.

**Por qué.** Es exactamente el mismo bloque de código Rust compartido; duplicar el parche en dos specs garantiza conflictos. La justificación completa (por qué el `/24` es incorrecto y qué lo reemplaza) está en Windows Fase 1.

**Cuidado con.** No re-implementes un filtro por subnet "mejorado" acá. Si querés un guard de alcanzabilidad, va en Windows Fase 1 para toda la plataforma, no un `#[cfg(target_os = "android")]` divergente.

---

### Tarea B.3 — Recibir archivos por streaming a MediaStore (eliminar el buffer del archivo completo)

**Problema.** Al terminar de recibir un archivo en Android, el handler lee el archivo entero del sandbox privado a un `Vec<u8>` y se lo pasa a `save_to_public_downloads(&bytes)`, que a su vez hace otra copia interna. Un archivo de 500 MB = 500 MB de RSS (o más). Además el `if let Ok(bytes)` no tiene `else`: si la lectura falla, el archivo queda en el sandbox privado **sin ningún log** y el usuario nunca lo ve.

**Archivo(s).** `src-tauri/src/http_server.rs:652-688` (el bloque `#[cfg(target_os = "android")]`) y `src-tauri/src/android_fs_bridge.rs:105-128` (`save_to_public_downloads`, que reemplazaremos por una versión de streaming).

**Estado actual.**
```rust
// http_server.rs:652
#[cfg(target_os = "android")]
{
    if let Ok(bytes) = tokio::fs::read(&target_path).await {
        let mime = file
            .rel_path
            .as_deref()
            .and_then(|p| mime_guess::from_path(p).first())
            .or_else(|| mime_guess::from_path(&file.name).first())
            .map(|m| m.essence_str().to_string());
        match crate::android_fs_bridge::save_to_public_downloads(
            &state.app,
            &file.name,
            &bytes,
            mime.as_deref(),
        )
        .await
        {
            Ok(public_uri) => {
                eprintln!("[http] published {} to public Downloads: {}", file.name, public_uri);
                let _ = tokio::fs::remove_file(&target_path).await;
            }
            Err(e) => {
                eprintln!("[http] could not publish {} to /Downloads: {} (file stays at {})",
                    file.name, e, target_path.display());
            }
        }
    }
    // <-- no hay else: si tokio::fs::read falla, silencio total.
}
```

Y el bridge actual (buffer completo):
```rust
// android_fs_bridge.rs:105
pub async fn save_to_public_downloads<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    bytes: &[u8],
    mime: Option<&str>,
) -> Result<String, String> {
    let api = app.android_fs_async();
    let _ = api.public_storage().request_permission().await;
    let uri = api.public_storage()
        .write_new(None, PublicGeneralPurposeDir::Download, filename, mime, bytes)
        .await
        .map_err(|e| format!("write_new downloads: {e}"))?;
    let _ = api.public_storage().scan(&uri).await;
    Ok(uri.uri)
}
```

**Cambio.** Agregar al bridge una función que **crea el destino MediaStore vacío, abre su file descriptor writable, y hace `std::io::copy` desde el archivo privado** — sin buffer intermedio. La API de `tauri-plugin-android-fs` v28.1.0 lo permite (todas confirmadas en el source de la crate):

- `api.public_storage().create_new_file_with_pending(volume_id, base_dir, relative_path, mime) -> Result<FileUri>` — crea el entry MediaStore marcado *pending* (invisible a otras apps hasta cerrar). Firma real: `create_new_file_with_pending(&self, Option<&StorageVolumeId>, impl Into<PublicDir>, impl AsRef<Path>, Option<&str>) -> Result<FileUri>`.
- `api.open_file_writable(&uri) -> Result<std::fs::File>` — devuelve un `std::fs::File` real (trunca el contenido). Es un método de `AndroidFs` (async vía `android_fs_async()`), no de `public_storage()`.
- `api.public_storage().set_pending(&uri, false)` — publica el archivo (lo hace visible). En Android 9 o menor es no-op.
- `api.public_storage().scan(&uri)` — para Android 9 o menor dispara el MediaScanner; en Android 10+ es no-op. Llamalo igual por seguridad legacy.

Nueva función en `android_fs_bridge.rs` (reemplaza a `save_to_public_downloads`):

```rust
/// Stream a finished file from a private filesystem path into the public
/// Downloads folder via MediaStore, WITHOUT buffering the whole file in
/// RAM. Returns the public URI string. The private source is NOT deleted
/// here — the caller decides.
pub async fn stream_file_to_public_downloads<R: Runtime>(
    app: &AppHandle<R>,
    src_path: &std::path::Path,
    filename: &str,
    mime: Option<&str>,
) -> Result<String, String> {
    let api = app.android_fs_async();
    let _ = api.public_storage().request_permission().await;

    // 1. Create the MediaStore destination (marked pending -> invisible).
    let uri = api
        .public_storage()
        .create_new_file_with_pending(
            None,
            PublicGeneralPurposeDir::Download,
            filename,
            mime,
        )
        .await
        .map_err(|e| format!("create_new_file downloads: {e}"))?;

    // 2. Open the destination FD writable and copy the source in chunks.
    let mut dst = api
        .open_file_writable(&uri)
        .await
        .map_err(|e| format!("open_file_writable: {e}"))?;
    let src_owned = src_path.to_path_buf();
    let copy_res = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<u64> {
        let mut src = std::fs::File::open(&src_owned)?;
        std::io::copy(&mut src, &mut dst) // 8 KiB internal buffer, no full-file alloc
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
    .map_err(|e| format!("copy: {e}"))?;

    // 3. Publish (Android 10+) and scan (Android 9 legacy).
    let _ = api.public_storage().set_pending(&uri, false).await;
    let _ = api.public_storage().scan(&uri).await;

    let _ = copy_res; // bytes copied, if you want to log it
    Ok(uri.uri)
}
```

Notas de tipos: `create_new_file_with_pending` toma `PublicGeneralPurposeDir::Download` (ya importado en el archivo, línea 25) directo — `impl Into<PublicDir>`. `open_file_writable` vive en la struct `AndroidFs` (accedida por `app.android_fs_async()`), no en `PublicStorage`; llamala como `api.open_file_writable(&uri)`.

Y en `http_server.rs:652`, reemplazar el bloque para (a) pasar `&target_path` en vez de leerlo, y (b) manejar el error del cierre `if let`:

```rust
#[cfg(target_os = "android")]
{
    let mime = file
        .rel_path
        .as_deref()
        .and_then(|p| mime_guess::from_path(p).first())
        .or_else(|| mime_guess::from_path(&file.name).first())
        .map(|m| m.essence_str().to_string());
    match crate::android_fs_bridge::stream_file_to_public_downloads(
        &state.app,
        &target_path,
        &file.name,
        mime.as_deref(),
    )
    .await
    {
        Ok(public_uri) => {
            eprintln!("[http] published {} to public Downloads: {}", file.name, public_uri);
            let _ = tokio::fs::remove_file(&target_path).await;
        }
        Err(e) => {
            eprintln!(
                "[http] could not publish {} to /Downloads: {} (file stays at {})",
                file.name, e, target_path.display()
            );
        }
    }
}
```

Si preferís mantener `save_to_public_downloads(&[u8])` para otros callers (no hay ninguno hoy fuera de `apply_update`, ver B.4), podés dejar la vieja y sólo agregar la nueva; pero lo más limpio es reemplazarla y actualizar el único otro caller.

**Por qué.** Elimina el pico de RAM proporcional al tamaño del archivo (la queja central de "quema RAM absurda" en Android). `std::io::copy` usa un buffer interno de 8 KiB; la RSS sube unos KB, no cientos de MB. El `create_new_file_with_pending` + `open_file_writable` es exactamente el patrón que la propia crate documenta para archivos grandes.

**Cuidado con.**
- El `event file-completed` (`http_server.rs:701`) sigue reportando `path: target_path` — está bien, pero recordá que tras el `remove_file` ese path privado ya no existe. Si el frontend intenta abrir ese `path`, en Android debería usar la `public_uri`. No lo cambies acá salvo que rompa; sólo dejá el `eprintln!` con la URI pública para diagnóstico.
- `open_file_writable` **trunca**: perfecto para un destino recién creado, pero no lo uses para resume.
- No borres `target_path` si `stream_file_to_public_downloads` devolvió `Err` — el archivo privado es el único respaldo. El código de arriba respeta esto (el `remove_file` está sólo en la rama `Ok`).
- `PublicGeneralPurposeDir` y `PublicImageDir` ya están importados (línea 24-26); no toques esos imports salvo que agregues tipos nuevos.
- `spawn_blocking` es obligatorio: `std::io::copy` sobre un `std::fs::File` es bloqueante y no debe correr en el runtime async de Tauri.

---

### Tarea B.4 — Escribir el APK de actualización por streaming (mismo patrón que B.3)

**Problema.** `apply_update` en Android descarga el APK a la caché, lo lee **entero** a un `Vec<u8>` (`tokio::fs::read(&staged)`), y se lo pasa a `save_to_public_downloads(&bytes)`. Un APK de 60–120 MB = ese tamaño en RAM, dos veces (lectura + copia interna del bridge).

**Archivo(s).** `src-tauri/src/lib.rs:893-931` (rama `#[cfg(target_os = "android")]` de `apply_update`).

**Estado actual.**
```rust
// lib.rs:904
let cache_dir = app.path().app_cache_dir()
    .map_err(|e| format!("resolve cache dir: {e}"))?;
let staged = updater::download_and_stage_apk(&download_url, &cache_dir)
    .await
    .map_err(|e| format!("{e:#}"))?;
let bytes = tokio::fs::read(&staged)
    .await
    .map_err(|e| format!("read staged apk: {e}"))?;
let _ = tokio::fs::remove_file(&staged).await;
let filename = format!(
    "Millennium Clipboard v{}.apk",
    updater::version_for_filename(&download_url)
);
let uri = android_fs_bridge::save_to_public_downloads(
    &app,
    &filename,
    &bytes,
    Some("application/vnd.android.package-archive"),
)
.await
.map_err(|e| format!("publish apk to Downloads: {e}"))?;
Ok(uri)
```

**Cambio.** Usar la nueva `stream_file_to_public_downloads` de B.3 y borrar el `staged` **después** del stream (no antes, o el stream no tendría de dónde leer):

```rust
let cache_dir = app.path().app_cache_dir()
    .map_err(|e| format!("resolve cache dir: {e}"))?;
let staged = updater::download_and_stage_apk(&download_url, &cache_dir)
    .await
    .map_err(|e| format!("{e:#}"))?;
let filename = format!(
    "Millennium Clipboard v{}.apk",
    updater::version_for_filename(&download_url)
);
let uri = android_fs_bridge::stream_file_to_public_downloads(
    &app,
    &staged,
    &filename,
    Some("application/vnd.android.package-archive"),
)
.await
.map_err(|e| format!("publish apk to Downloads: {e}"))?;
// Cleanup AFTER the stream copied it out.
let _ = tokio::fs::remove_file(&staged).await;
Ok(uri)
```

**Por qué.** Mismo motivo que B.3: el APK ya está en disco (staged); no hay razón para traerlo entero a RAM sólo para reenviarlo a MediaStore. `std::io::copy` disco→MediaStore es constante en memoria.

**Cuidado con.**
- El orden importa: `remove_file(&staged)` **después** del `stream_file_to_public_downloads`, no antes. El código original lo borraba antes de escribir la copia pública (funcionaba sólo porque ya tenía los bytes en RAM).
- No cambies `download_and_stage_apk` ni `version_for_filename`; siguen igual.
- La rama Windows de `apply_update` (`lib.rs:884-892`) no se toca.

---

### Tarea B.5 — Endurecer el path de envío por SAF (`resolve_content_uri`)

**Problema.** Cuando el usuario elige un archivo por el picker SAF, `resolve_content_uri` copia el `content://` a `<cache>/uploads/<displayName>`. Tres bugs: (1) el nombre de caché es el `displayName` crudo — dos archivos distintos con el mismo nombre (p.ej. dos `IMG_0001.jpg` de álbumes distintos) colisionan; (2) la copia de caché nunca se borra tras el upload → la caché crece sin límite; (3) el caller (`send_files` en `lib.rs:210`) hace `tokio::fs::metadata` sobre el path devuelto, pero como el archivo de caché se crea por streaming, si `get_name`/copia dan un archivo de 0 bytes por una race, la cola muestra `0 B`. Statear el archivo real y esperar a que la copia termine evita el `0 B`.

**Archivo(s).** `src-tauri/src/android_fs_bridge.rs:31-67` (`resolve_content_uri`); caller en `src-tauri/src/lib.rs:958-965` (`prepare_file_for_send`) y consumidor del tamaño en `src-tauri/src/lib.rs:210-219`.

**Estado actual.**
```rust
// android_fs_bridge.rs:31
pub async fn resolve_content_uri<R: Runtime>(
    app: &AppHandle<R>,
    uri_str: String,
) -> Result<PathBuf, String> {
    let api = app.android_fs_async();
    let uri = FileUri::from_uri(uri_str);
    let name = api.get_name(&uri).await.map_err(|e| format!("get_name: {e}"))?;
    let cache_dir = api.private_storage()
        .resolve_path(PrivateDir::Cache).await
        .map_err(|e| format!("resolve_path cache: {e}"))?
        .join("uploads");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir cache: {e}"))?;
    let dest = cache_dir.join(&name);
    let mut src = api.open_file_readable(&uri).await
        .map_err(|e| format!("open_file_readable: {e}"))?;
    let dest_clone = dest.clone();
    tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
        let mut out = std::fs::File::create(&dest_clone)?;
        std::io::copy(&mut src, &mut out)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
    .map_err(|e| format!("copy: {e}"))?;
    Ok(dest)
}
```

**Cambio.** (a) Sufijar el nombre de caché con un UUID corto para evitar colisiones **preservando la extensión y el nombre visible** (el nombre que ve el receptor sale de otro lado — el `PrepareFile.name` en `lib.rs:220` usa `p.file_name()` del path de caché, así que hay que preservar el nombre base y sólo insertar el UUID en el *directorio*, no en el filename, para que el receptor no vea `IMG_0001-abcd1234.jpg`). La forma más limpia: poner cada copia en un **subdirectorio único** `uploads/<uuid>/<name>`. Así el `file_name()` sigue siendo `name` limpio y no hay colisión. (b) Devolver el tamaño real y (c) borrar la copia tras el upload.

Para (c), el borrado no puede pasar dentro de `resolve_content_uri` (el upload ocurre después, en `send_files`). Dos opciones — elegí la más simple para el parque de código:

- **Opción recomendada:** `resolve_content_uri` sólo arregla (a) y (b); el borrado de la copia lo hace `send_files` tras terminar el upload de ese archivo. En `lib.rs:268-...` el loop de upload ya itera `upload_plan` con el `path`; agregar, en Android, un `tokio::fs::remove_file(path)` (o borrar el subdir `uploads/<uuid>/`) después del `upload_file(...)` exitoso, detrás de `#[cfg(target_os = "android")]`.

Sketch de `resolve_content_uri` con subdir único:

```rust
pub async fn resolve_content_uri<R: Runtime>(
    app: &AppHandle<R>,
    uri_str: String,
) -> Result<PathBuf, String> {
    let api = app.android_fs_async();
    let uri = FileUri::from_uri(uri_str);
    let name = api.get_name(&uri).await.map_err(|e| format!("get_name: {e}"))?;

    // Unique per-file subdir so two files with the same displayName
    // (e.g. two IMG_0001.jpg) don't collide, WITHOUT mangling the name
    // the receiver will see (send_files derives it from file_name()).
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let cache_dir = api.private_storage()
        .resolve_path(PrivateDir::Cache).await
        .map_err(|e| format!("resolve_path cache: {e}"))?
        .join("uploads")
        .join(&unique);
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir cache: {e}"))?;
    let dest = cache_dir.join(&name);

    let mut src = api.open_file_readable(&uri).await
        .map_err(|e| format!("open_file_readable: {e}"))?;
    let dest_clone = dest.clone();
    let copied = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<u64> {
        let mut out = std::fs::File::create(&dest_clone)?;
        std::io::copy(&mut src, &mut out) // returns bytes copied
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
    .map_err(|e| format!("copy: {e}"))?;

    if copied == 0 {
        crate::runtime_log::warn(format!(
            "[android-fs] resolved {} copied 0 bytes (queue will show 0 B)",
            name
        ));
    }
    Ok(dest)
}
```

Sobre (b) — el `0 B`: el `send_files` en `lib.rs:210` hace `tokio::fs::metadata(&p).await` sobre el path devuelto. Como `resolve_content_uri` ya hace `.await` del `spawn_blocking` **antes** de devolver, el archivo está completo cuando `metadata` corre — el `0 B` que se veía era por colisión (dos archivos escribiendo el mismo `dest`, uno truncando al otro), que el subdir único elimina. El chequeo `copied == 0` es un guard de diagnóstico, no un fix por sí mismo.

Para el borrado (Opción recomendada), en el loop de upload de `send_files` (`lib.rs:268`), tras el `upload_file(...)` de cada archivo:

```rust
#[cfg(target_os = "android")]
{
    // path apunta a <cache>/uploads/<uuid>/<name>; borrar el subdir uuid.
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::remove_dir_all(parent).await;
    }
}
```

Confirmá que `uuid` ya está en `Cargo.toml` (lo está — `send_files` usa `Uuid::new_v4()` en `lib.rs:204`).

**Por qué.** El subdir único mata las colisiones sin ensuciar el nombre que ve el receptor. Borrar la copia tras el upload evita que la caché privada crezca sin techo (relevante en un teléfono con poco espacio). El guard `copied == 0` da una pista en el log cuando el `0 B` reaparezca por otra causa (p.ej. un `content://` que el proveedor no deja leer).

**Cuidado con.**
- No metas el UUID en el **nombre del archivo** (`dest = cache_dir.join(format!("{unique}-{name}"))`) — eso haría que el receptor reciba `abcd1234-IMG_0001.jpg`, porque `send_files` toma el nombre de `p.file_name()`. Por eso el UUID va en el **directorio**.
- Al borrar `remove_dir_all(parent)`, asegurate de que `parent` sea el subdir `<uuid>`, no `uploads/` entero. El sketch usa `path.parent()` que es correcto sólo si el layout es `uploads/<uuid>/<name>`.
- Sólo borrá tras un upload **exitoso**. Si `upload_file` falló y hay reintento/resume, no borres la fuente todavía. Revisá el flujo de error de `send_files` (`lib.rs:268-...`) antes de mover el `remove_dir_all`.

---

### Tarea B.6 — Limpiar `network_security_config.xml` (las entradas IP-como-`<domain>` no hacen nada)

**Problema.** El config declara IPs de red (`10.0.0.0`, `192.168.0.0`, `172.16.0.0`) como `<domain>`. NSC **no** interpreta CIDR ni rangos: un `<domain>` matchea un host exacto por nombre/IP literal, no una subred. `10.0.0.0` sólo matchearía un peer cuya IP sea literalmente `10.0.0.0` (nunca). Estas entradas dan una falsa sensación de que "el cleartext a la LAN está permitido"; en realidad no cubren ningún peer real. Y no hace falta que lo cubran: el tráfico entre peers es TLS nativo vía `rustls`/`reqwest` con verificación por fingerprint SHA-256, que **no pasa por la validación de NSC del WebView**.

**Archivo(s).** `src-tauri/gen/android/app/src/main/res/xml/network_security_config.xml:22-33`.

**Estado actual.**
```xml
<!-- Private IPv4 ranges where peers live. -->
<domain-config cleartextTrafficPermitted="true">
    <domain includeSubdomains="false">10.0.0.0</domain>
    <domain includeSubdomains="false">192.168.0.0</domain>
    <domain includeSubdomains="false">172.16.0.0</domain>
    <domain includeSubdomains="false">127.0.0.1</domain>
    <domain includeSubdomains="false">localhost</domain>
    <trust-anchors>
        <certificates src="system" />
        <certificates src="user" />
    </trust-anchors>
</domain-config>
```

**Cambio.** Borrar las tres entradas de IP-de-red (que no matchean nada) y quedarse sólo con `localhost`/`127.0.0.1` (que sí son hosts literales válidos, por si el WebView alguna vez habla cleartext con un servidor local). Reescribir el comentario para documentar que el tráfico peer es rustls nativo y no depende de NSC:

```xml
<!--
  El tráfico entre peers es TLS nativo (rustls/reqwest con verificación
  por fingerprint SHA-256) y NO pasa por la validación NSC del WebView,
  así que no hace falta declarar las subredes LAN acá. NSC además NO
  entiende CIDR: un <domain> con "10.0.0.0" sólo matchearía ese host
  literal (nunca un peer real), por eso se removieron esas entradas.

  Sólo dejamos cleartext para localhost por si el WebView alguna vez
  habla con un servidor local sin TLS.
-->
<domain-config cleartextTrafficPermitted="true">
    <domain includeSubdomains="false">127.0.0.1</domain>
    <domain includeSubdomains="false">localhost</domain>
    <trust-anchors>
        <certificates src="system" />
        <certificates src="user" />
    </trust-anchors>
</domain-config>
```

Mantené el `<base-config cleartextTrafficPermitted="false">` de arriba (líneas 15-20) intacto.

**Por qué.** Elimina configuración muerta que confunde a quien la lea (parece que habilita la LAN pero no habilita nada). Documenta explícitamente que la seguridad de transporte peer-a-peer vive en Rust, no en NSC.

**Cuidado con.**
- No pongas `cleartextTrafficPermitted="true"` en el `base-config`: eso abriría cleartext global y es justo lo que el comentario original (líneas 2-13) buscaba evitar.
- Este archivo está bajo `gen/android/` (generado por `tauri android init`). Confirmá en `../00-SHARED-CONTEXT.md` si `gen/android` está bajo control de versiones o se regenera; si se regenera, este cambio debe ir en el template/hook correspondiente, no sólo en el archivo generado (o se perderá en el próximo `android init`).

---

## Cómo verificar

Comandos de build/run/deploy: ver `../00-SHARED-CONTEXT.md`. Verificaciones específicas de esta fase:

1. **B.1 (Wi-Fi IP):** con el teléfono en Wi-Fi + datos móviles a la vez, arrancá la app y mirá el runtime log (endpoint/comando `get_runtime_log`, ver shared context). Debe aparecer:
   - `[net] N IPv4 interface(s) detected:` seguido de la lista, incluyendo `wlan0 -> 192.168.x.y` (o `10.x`).
   - `[diag] identity ... local_ip=192.168.x.y` con la IP de `wlan0`, **no** una IP celular (`10.x` de rmnet suele ser CGNAT del operador; distinguila mirando el nombre de interfaz en la lista).
   - Si apagás Wi-Fi: debe aparecer `[net] no usable Wi-Fi/LAN IPv4 address found` y `[net] identity.local_ip is empty ...`.
   - En Windows (multi-NIC con WSL/Docker): `[diag] identity ... local_ip=` debe ser la NIC física, no `vEthernet`.
2. **B.3 (streaming recibir):** en el teléfono, `adb shell dumpsys meminfo <applicationId>` justo antes de recibir un archivo grande (≥300 MB) y de nuevo durante la recepción. El `TOTAL PSS` no debe subir en el orden del tamaño del archivo (unos MB de chunk, no cientos). Log esperado: `[http] published <name> to public Downloads: content://...`. Verificá con un explorador de archivos que el archivo está en `Downloads/` y que **no** quedó copia en `Android/data/<pkg>/`. Forzá un fallo (p.ej. sin permiso de storage) y confirmá que ahora hay log `[http] could not publish ...` (antes: silencio).
3. **B.4 (streaming APK):** disparar una actualización; `dumpsys meminfo` no debe mostrar un pico del tamaño del APK. El APK aparece en `Downloads/Millennium Clipboard vX.Y.Z.apk`.
4. **B.5 (SAF send):** elegir por el picker dos archivos distintos que se llamen igual (renombrá dos fotos a `same.jpg` en álbumes distintos) y mandarlos en la misma sesión. Ambos deben llegar completos y con su contenido correcto (no uno pisando al otro). La cola de envío debe mostrar el tamaño real, no `0 B`. Tras el envío, `adb shell ls Android/data/<pkg>/cache/uploads/` debe estar vacío (o sin los subdirs de esa sesión).
5. **B.6 (NSC):** el build de Android compila. `aapt dump xmltree` sobre el APK (o inspección del `res/xml/network_security_config.xml` empaquetado) ya no muestra los `<domain>` `10.0.0.0`/`192.168.0.0`/`172.16.0.0`. Una transferencia peer-a-peer sigue funcionando (prueba de que no dependía de esas entradas).

**Test unitario a agregar (B.1):** en `identity.rs`, un `#[test]` para `is_rfc1918` y para el scoring de `resolve_lan_ipv4`. Como `list_afinet_netifas` toca el SO, extraé la lógica de scoring a una función pura testeable, p.ej. `fn pick_best_ipv4(ifaces: &[(String, Ipv4Addr)]) -> Option<Ipv4Addr>`, y testeala:

```rust
#[test]
fn prefers_wlan_over_cellular() {
    let ifaces = vec![
        ("rmnet_data0".to_string(), "10.120.4.7".parse().unwrap()),   // celular CGNAT
        ("wlan0".to_string(),       "192.168.1.42".parse().unwrap()), // Wi-Fi
    ];
    assert_eq!(pick_best_ipv4(&ifaces), Some("192.168.1.42".parse().unwrap()));
}

#[test]
fn skips_virtual_and_loopback() {
    let ifaces = vec![
        ("lo".to_string(),              "127.0.0.1".parse().unwrap()),
        ("vEthernet (WSL)".to_string(), "172.20.0.1".parse().unwrap()),
        ("Ethernet".to_string(),        "192.168.0.10".parse().unwrap()),
    ];
    assert_eq!(pick_best_ipv4(&ifaces), Some("192.168.0.10".parse().unwrap()));
}
```

Corré la suite con el comando de tests del `../00-SHARED-CONTEXT.md`.

## Riesgo y rollback

- **B.1** es el cambio de mayor alcance: toca la IP que usa TODO el descubrimiento en ambas plataformas. Riesgo: un scoring mal calibrado elige una NIC peor que la del comportamiento actual en algún setup raro. Mitigación: el log `[net] ... -> ...` ya imprime todas las NICs, así que un mal pick es diagnosticable de inmediato. Rollback: restaurar el `compute_local_ip` de 4 líneas (usa `local_ip_address::local_ip()`); es un revert aislado que no afecta a B.3/B.4/B.5/B.6.
- **B.3 / B.4** comparten la nueva `stream_file_to_public_downloads`. Riesgo: que `open_file_writable` sobre el `FileUri` de `create_new_file_with_pending` falle en alguna versión de Android; el `Err` se loguea y el archivo queda en el sandbox privado (mismo estado de "no publicado" que un fallo del código viejo, pero ahora con log). Rollback: volver a `save_to_public_downloads(&bytes)` (mantené la función vieja hasta confirmar B.3/B.4 en device real). Son shippables independientes de B.1/B.5/B.6.
- **B.5** sólo afecta el path de envío SAF. Riesgo: el borrado `remove_dir_all` tras un upload podría correr antes de un resume si el flujo de reintento reusa la copia; por eso hay que ponerlo sólo en la rama de éxito. Rollback: quitar el sufijo UUID y el `remove_dir_all`; independiente del resto.
- **B.6** es cosmético/config. Riesgo casi nulo (las entradas removidas no matcheaban nada). Rollback: restaurar las tres líneas `<domain>`. Ojo con el punto sobre `gen/android` regenerable en "Cuidado con" — si se regenera, el cambio va en el template.
- **B.2** no es un cambio en este spec; su riesgo/rollback vive en Windows Fase 1, Tarea 1.3.
