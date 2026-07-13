# Millennium Clipboard — Android · Fase A: Ciclo de vida del servicio + aprobación nativa

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Android · **Prerrequisitos:** ninguno (esta es la primera fase Android, la de mayor palanca) · **Esfuerzo:** ~2-3 días · **Riesgo:** high (toca el ciclo de vida del proceso y el foreground service; una regresión deja Android sin recibir nada)

## Objetivo

Hacer que Android sea un peer honesto y funcional cuando la app está en background o con la pantalla apagada. Hoy el `MillenniumService` es solo un extensor de vida de proceso que **no hospeda el runtime de Rust**, devuelve `START_STICKY` (el sistema lo revive como cascarón vacío con una notificación "Linked" mentirosa), usa un `foregroundServiceType` equivocado (`dataSync`, con tope de 6 h en Android 15+), y la aprobación de archivos entrantes depende de un modal en el WebView que está congelado cuando la Activity no está en primer plano. Esta fase corrige el tipo de FGS, elimina el zombie, y crea una ruta de aprobación **nativa** (notificación con botones Accept/Decline) que resuelve el `oneshot` de Rust sin depender del WebView.

## Definición de "hecho"

- [ ] El manifest declara `android:foregroundServiceType="connectedDevice"` y la permission `FOREGROUND_SERVICE_CONNECTED_DEVICE`.
- [ ] `MillenniumService` sobreescribe `onTimeout(startId, fgsType)` y hace `stopSelf()` sin crashear.
- [ ] Tras un kill del sistema, **no** queda una notificación "Linked" sobre un proceso muerto: o el servicio hospeda el core y realmente recibe, o el servicio se detiene y la notificación desaparece.
- [ ] Con la Activity en background y la pantalla apagada, enviar archivos desde un peer de escritorio hace aparecer una **notificación nativa** con botones "Aceptar" y "Rechazar"; tocar "Aceptar" completa la transferencia; tocar "Rechazar" la corta con `403`; ignorarla la deja expirar por `APPROVAL_TIMEOUT`.
- [ ] Con la Activity en primer plano, sigue apareciendo el modal del WebView (fallback), sin doble prompt.
- [ ] En Android 13+ (`SDK_INT >= 33`), al primer arranque se pide `POST_NOTIFICATIONS` **antes** de arrancar el servicio, y la notificación FGS es visible.
- [ ] Se pide una vez `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; un peer "visto hace poco pero callado" no se elimina del peer list, se marca reachable-on-demand.

## Tareas

### Tarea A.1 — Foreground service type correcto + red de seguridad `onTimeout`

**Problema.** El manifest declara `foregroundServiceType="dataSync"` y la permission `FOREGROUND_SERVICE_DATA_SYNC`. `dataSync` en Android 15 (API 35) tiene un **tope acumulado de ~6 h por día**; al pasarlo el sistema llama `Service.onTimeout()` y, si no lo manejás, mata el proceso con un ANR/crash. Un servicio de comunicación con peers en la LAN encaja en `connectedDevice`, que no tiene ese tope.

**Archivo(s).** `src-tauri/gen/android/app/src/main/AndroidManifest.xml:15`, `:59-62` · `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MillenniumService.kt:50-53`

**Estado actual.**

Manifest (permission, línea 15):
```xml
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_DATA_SYNC" />
```

Manifest (declaración del service, líneas 55-62):
```xml
        <!-- Foreground service that keeps mDNS/UDP/HTTPS alive while
             the app is in the background. The dataSync foreground type
             is the closest fit for a peer-sync app per Android 14+
             docs. -->
        <service
          android:name=".MillenniumService"
          android:exported="false"
          android:foregroundServiceType="dataSync" />
```

Service (`onStartCommand`, líneas 50-53):
```kotlin
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // START_STICKY so Android restarts us if the OS reclaims memory.
        return START_STICKY
    }
```

**Cambio.**

1. En el manifest, reemplazá la permission `FOREGROUND_SERVICE_DATA_SYNC` por `FOREGROUND_SERVICE_CONNECTED_DEVICE` (dejá intacta `FOREGROUND_SERVICE`):
```xml
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />
```

2. En la declaración del `<service>`, cambiá el tipo y actualizá el comentario:
```xml
        <!-- Foreground service that keeps mDNS/UDP/HTTPS alive while
             the app is in the background. connectedDevice is the correct
             type for LAN peer comms and has no daily runtime cap
             (dataSync is capped at ~6h/day on Android 15+). -->
        <service
          android:name=".MillenniumService"
          android:exported="false"
          android:foregroundServiceType="connectedDevice" />
```

3. En `MillenniumService.kt`, sobreescribí `onTimeout` como red de seguridad. Android 14 (API 34) introdujo `onTimeout(startId: Int)`; Android 15 (API 35) agregó la sobrecarga `onTimeout(startId: Int, fgsType: Int)`. `connectedDevice` **no** debería disparar timeout, pero implementamos el handler para no crashear si el OEM lo aplica igual. Agregá, dentro de la clase (por ejemplo después de `onStartCommand`):
```kotlin
    // Safety net: on Android 14+ the system may call onTimeout for some
    // FGS types. connectedDevice has no daily cap, but if a vendor build
    // still fires it we must stop gracefully instead of getting killed
    // with an ANR. Overriding both signatures keeps us correct across
    // API 34 (single-arg) and API 35+ (two-arg).
    override fun onTimeout(startId: Int) {
        stopSelf()
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        stopSelf()
    }
```

**Por qué.** `connectedDevice` describe con exactitud lo que hace la app (comunicación con dispositivos en la misma red) y evita el tope diario que mataría el servicio en sesiones largas. `onTimeout` evita que un timeout inesperado se convierta en un crash visible.

**Cuidado con.** El `<service>` está en un manifest **generado** por Tauri (`gen/android/...`). Verificá que Tauri no lo regenere sobrescribiendo tus cambios (ver `../00-SHARED-CONTEXT.md` para el flujo de build Android); si lo hace, el cambio del `<service>`/permissions debe ir en la plantilla de merge de Tauri, no solo en el archivo generado. El método `onTimeout(startId: Int)` de un solo argumento requiere `compileSdk >= 34` (el proyecto tiene `compileSdk = 36`, ver `build.gradle.kts:26`, así que compila). No borres `FOREGROUND_SERVICE` (la genérica sigue siendo obligatoria).

---

### Tarea A.2 — Eliminar el zombie `START_STICKY`

**Problema.** El servicio devuelve `START_STICKY`, así que si el sistema recupera memoria y mata el proceso, Android lo **recrea** — llama `onCreate()` → `startForeground()` con la notificación "Linked — receiving transfers in the background". Pero el runtime de Rust (server HTTPS + discovery) **no vive en el servicio**: se arranca desde `setup()` en `lib.rs` (líneas 1306-1386, dentro del `mobile_entry_point`/`run()`), que corre en el proceso de la Activity. El servicio es puro extensor de vida (comentario en `MillenniumService.kt:12-15`). Resultado: tras la resurrección hay una notificación que promete recepción sobre un proceso sin server ni sockets. El peer de escritorio ve el phone "online" por la notificación pero cualquier `/prepare-upload` falla.

**Archivo(s).** `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MillenniumService.kt:50-53` · contexto en `src-tauri/src/lib.rs:1306-1386`, `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MainActivity.kt:13-20`

**Estado actual.** El servicio no carga la native lib ni arranca nada de Rust; solo toma el `MulticastLock` y muestra la notificación (`MillenniumService.kt:43-53`). El core Rust arranca en la Activity vía `run()` (`lib.rs:982`, `#[cfg_attr(mobile, tauri::mobile_entry_point)]`). La Activity dispara el servicio en `onCreate` (`MainActivity.kt:15-20`).

**Cambio.** Hay dos opciones. **Especificá ambas; recomendá la Opción B para esta fase** y dejá la Opción A documentada como trabajo futuro.

**Opción A — el servicio hospeda un core Rust headless.** El servicio carga la native lib (`System.loadLibrary("millennium_clipboard_lib")`), arranca server + discovery **independiente de la Activity**, y la Activity pasa a ser un visor delgado que se conecta al mismo runtime. `START_STICKY` se vuelve honesto porque la resurrección del servicio realmente relevanta el core.
- **Costo real:** hoy TODO el bootstrap (identity, settings store, HTTPS server, mDNS, UDP, `app.manage(AppState)`) vive dentro del closure `.setup()` de Tauri (`lib.rs:1037-1408`) y depende de un `AppHandle` de Tauri (`app.path()`, `app.emit`, `app.android_fs_async()` en `android_fs_bridge.rs`). No existe hoy un "core" desacoplable de Tauri. Extraerlo a una función `start_headless_core()` invocable por JNI desde el servicio es una refactorización grande y arriesgada (duplicaría o movería la inicialización de `AppState`, y `tauri-plugin-android-fs` necesita el runtime de Tauri). **No apto para esta fase.**

**Opción B (RECOMENDADA) — decir la verdad: `START_NOT_STICKY` + `stopSelf()` cuando no hay runtime.** El servicio no hospeda el core, así que no debe fingir que sobrevive al proceso. Devolvé `START_NOT_STICKY` para que Android **no** recree un cascarón vacío; y como el servicio y el core comparten proceso, cuando el proceso muere el servicio muere con él y la notificación desaparece honestamente. Además, protegé contra el arranque "en frío" del servicio sin Activity (p. ej. una recreación heredada): si el servicio se crea sin que el core esté vivo, `stopSelf()`.

Pasos concretos (Opción B):

1. En `MillenniumService.kt`, cambiá el retorno de `onStartCommand`:
```kotlin
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // START_NOT_STICKY: the Rust runtime (HTTPS server + discovery)
        // lives in the Activity's process, NOT in this service. If the OS
        // kills the process, resurrecting an empty service would show a
        // "Linked" notification over a dead core. We'd rather the
        // notification honestly disappear. The Activity re-launches this
        // service in onCreate when the user reopens the app.
        return START_NOT_STICKY
    }
```

2. Agregá un flag estático que la Activity setea cuando el core está vivo, y consultalo en `onCreate` del servicio para no quedar como cascarón. En `MillenniumService.kt`, dentro del `companion object`:
```kotlin
    companion object {
        private const val CHANNEL_ID = "millennium_fg"
        private const val NOTIF_ID = 7777
        private const val MULTICAST_LOCK_TAG = "MillenniumMulticast"

        // Set true by MainActivity once the Tauri/Rust core has booted in
        // this process. If the service is (re)created without a live core
        // — e.g. a stale STICKY relaunch inherited from an old build — it
        // stops itself instead of showing a lying notification.
        @Volatile @JvmStatic var coreAlive: Boolean = false
    }
```
En `onCreate` del servicio, después de `super.onCreate()` y **antes** de `startForeground`, salí temprano si el core no está vivo:
```kotlin
    override fun onCreate() {
        super.onCreate()
        if (!coreAlive) {
            // No Rust runtime in this process — don't post a false
            // "receiving transfers" notification. Just stop.
            stopSelf()
            return
        }
        ensureChannel()
        acquireMulticastLock()
        startForeground(NOTIF_ID, buildNotification())
    }
```

3. En `MainActivity.kt`, seteá `coreAlive = true` **antes** de arrancar el servicio (el core ya arrancó en `super.onCreate()` porque `TauriActivity` ejecuta el `mobile_entry_point`):
```kotlin
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)   // Tauri boots the Rust core here

    MillenniumService.coreAlive = true
    // ... (start service, see Tarea A.4 for the permission gate)
  }
```

**Por qué.** La notificación FGS es un contrato con el usuario: dice "estoy recibiendo transferencias". Con `START_NOT_STICKY` + el chequeo `coreAlive`, esa promesa solo se muestra cuando es cierta. Opción A es la solución "correcta" a largo plazo (recepción real con app cerrada) pero requiere desacoplar el core de Tauri, lo cual excede esta fase y es de alto riesgo.

**Cuidado con.** `coreAlive` es best-effort dentro de un proceso; si el proceso muere, el flag se reinicia a `false` naturalmente (es memoria del proceso), que es justo lo que querés. No lo persistas en disco. No cambies el hecho de que la Activity arranca el servicio en `onCreate` — solo agregá el flag antes. Si en el futuro se hace la Opción A, este chequeo `coreAlive` se elimina y `START_STICKY` vuelve a ser válido.

---

### Tarea A.3 — Aprobación nativa con notificación (WebView congelado)

**Problema.** Cuando llega `/prepare-upload`, `handle_prepare_upload` (`http_server.rs:302-425`) emite el evento `incoming-files-request` al frontend y **bloquea en un `oneshot`** hasta `APPROVAL_TIMEOUT` (`http_server.rs:37`, 60 s) esperando que el usuario toque Aceptar/Rechazar en el modal del WebView, que llama `approve_session`/`reject_session` (`lib.rs:290-306`) → `http_server::resolve_approval` (`http_server.rs:265-274`). Pero cuando la Activity está en background el WebView está congelado: el evento se emite al vacío, nadie llama `resolve_approval`, y **toda transferencia expira a los 60 s**. Android es inutilizable con la pantalla apagada.

**Archivo(s).** `src-tauri/src/http_server.rs:346-369` (emisión + espera), `:265-274` (`resolve_approval`) · `src-tauri/src/lib.rs:290-306` (comandos) · nuevo módulo `src-tauri/src/android_notify.rs` · nuevo/editado `MillenniumService.kt` + un `BroadcastReceiver` · `mobile.json`

**Estado actual.**

`http_server.rs:346-369` — emite y espera solo por el WebView:
```rust
    let _ = state.app.emit("incoming-files-request", &preview);

    let approved = if auto_accept {
        true
    } else {
        // Wait for user decision via approve_session / reject_session command.
        let (tx, rx) = oneshot::channel::<bool>();
        approval_registry()
            .lock()
            .unwrap()
            .insert(req.session_id.clone(), tx);

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                approval_registry().lock().unwrap().remove(&req.session_id);
                let _ = state.app.emit(
                    "incoming-files-timeout",
                    serde_json::json!({ "sessionId": req.session_id }),
                );
                false
            }
        }
    };
```

`http_server.rs:265-274` — el resolvedor del `oneshot` (esto ya es el punto de entrada perfecto; la notificación nativa debe terminar llamándolo):
```rust
pub fn resolve_approval(session_id: &str, approved: bool) -> bool {
    let reg = approval_registry();
    let tx = { reg.lock().unwrap().remove(session_id) };
    if let Some(tx) = tx {
        let _ = tx.send(approved);
        true
    } else {
        false
    }
}
```

**Cambio.** Diseño: cuando la app **no** está en primer plano, además de emitir `incoming-files-request` (para el fallback WebView si vuelve al foreground), Rust dispara una **notificación nativa** con acciones Accept/Decline cuyos `PendingIntent` vuelven a Rust y llaman `resolve_approval(session_id, decision)`. La ruta de retorno más simple y sin nuevas dependencias es un `BroadcastReceiver` en Kotlin que reciba el tap, y una función `#[no_mangle] extern "C"` en Rust llamada por JNI desde ese receiver. La comparto abajo en tres piezas.

**(a) Rust — disparar la notificación cuando estamos en background.**

Creá `src-tauri/src/android_notify.rs`. Reutiliza el canal FGS `"millennium_fg"` (mismo `CHANNEL_ID` que `MillenniumService.kt:34`). Se invoca a Kotlin vía JNI usando el `JavaVM` que Tauri/ndk-context expone. Usá el crate `jni` (agregarlo a Cargo, ver más abajo) y `ndk-context::android_context()` para obtener el `JavaVM` y el `Context` de la app.

```rust
// src-tauri/src/android_notify.rs
#![cfg(target_os = "android")]

use jni::objects::{JObject, JValue};
use jni::JavaVM;

/// Post the native Accept/Decline notification for a pending incoming
/// transfer. Calls into Kotlin: MillenniumService.postApprovalNotification.
/// session_id is the key used later by resolve_approval.
pub fn post_approval_notification(
    session_id: &str,
    sender_alias: &str,
    file_count: usize,
    total_size: u64,
) {
    if let Err(e) = post_impl(session_id, sender_alias, file_count, total_size) {
        crate::runtime_log::warn(format!("[android_notify] post failed: {e}"));
    }
}

fn post_impl(
    session_id: &str,
    sender_alias: &str,
    file_count: usize,
    total_size: u64,
) -> anyhow::Result<()> {
    // ndk-context is a transitive dep of tauri on Android; it exposes the
    // process-wide JavaVM + Android Context set up by the NativeActivity.
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast())? };
    let mut env = vm.attach_current_thread()?;
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let j_session = env.new_string(session_id)?;
    let j_alias = env.new_string(sender_alias)?;

    env.call_static_method(
        "com/guidocameraeq/millennium/MillenniumService",
        "postApprovalNotification",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;IJ)V",
        &[
            JValue::Object(&context),
            JValue::Object(&j_session),
            JValue::Object(&j_alias),
            JValue::Int(file_count as i32),
            JValue::Long(total_size as i64),
        ],
    )?;
    Ok(())
}

/// Called by JNI from the Kotlin BroadcastReceiver when the user taps
/// Accept/Decline on the native notification. Resolves the oneshot.
#[no_mangle]
pub extern "C" fn Java_com_guidocameraeq_millennium_ApprovalReceiver_nativeResolveApproval(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    session_id: jni::objects::JString,
    approved: jni::sys::jboolean,
) {
    let sid: String = match env.get_string(&session_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let ok = crate::http_server::resolve_approval(&sid, approved != 0);
    crate::runtime_log::info(format!(
        "[android_notify] native decision session={sid} approved={} resolved={ok}",
        approved != 0
    ));
}
```

Declará el módulo en `lib.rs` junto a los otros `#[cfg(target_os = "android")]` (cerca de la línea 27, al lado de `mod android_fs_bridge;`):
```rust
#[cfg(target_os = "android")]
mod android_notify;
```

Agregá a `Cargo.toml`, en el bloque `[target.'cfg(target_os = "android")'.dependencies]` (donde ya está `tauri-plugin-android-fs`):
```toml
jni = "0.21"
ndk-context = "0.1"
```

**(b) Rust — llamar a la notificación desde `handle_prepare_upload`.** Modificá la rama del `else` para disparar la notificación nativa antes de esperar el `oneshot`. En Android disparás siempre la nativa (es la ruta confiable); el `emit` del WebView queda como fallback si la Activity vuelve. Reemplazá el bloque `else { ... }` de `http_server.rs:350-369` por:
```rust
    } else {
        // Register the oneshot first so both the WebView modal AND the
        // native notification resolve into the same channel.
        let (tx, rx) = oneshot::channel::<bool>();
        approval_registry()
            .lock()
            .unwrap()
            .insert(req.session_id.clone(), tx);

        // Native path: post an Android notification with Accept/Decline
        // buttons that call back into Rust even while the WebView is
        // frozen. The WebView emit above is the foreground fallback.
        #[cfg(target_os = "android")]
        crate::android_notify::post_approval_notification(
            &req.session_id,
            &req.sender_alias,
            req.files.len(),
            total,
        );

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                approval_registry().lock().unwrap().remove(&req.session_id);
                let _ = state.app.emit(
                    "incoming-files-timeout",
                    serde_json::json!({ "sessionId": req.session_id }),
                );
                #[cfg(target_os = "android")]
                crate::android_notify::cancel_approval_notification(&req.session_id);
                false
            }
        }
    };
```
Y agregá en `android_notify.rs` un `cancel_approval_notification(session_id: &str)` análogo (mismo patrón JNI, llamando a un `MillenniumService.cancelApprovalNotification(context, session_id)` en Kotlin que hace `NotificationManager.cancel(notifId)`), para retirar la notificación cuando expira o cuando el WebView ya decidió. Nota: el `resolve_approval` que dispara el WebView (via `approve_session`/`reject_session` en `lib.rs:290-306`) NO retira la notificación nativa; agregá esa llamada a `cancel_approval_notification` dentro de esos dos comandos, envuelta en `#[cfg(target_os = "android")]`, para evitar una notificación huérfana tras decidir desde el modal.

**(c) Kotlin — postear la notificación con acciones + el `BroadcastReceiver`.** Agregá a `MillenniumService.kt` (métodos estáticos en el `companion object`) y creá `ApprovalReceiver.kt`. El `notifId` deriva del `session_id` (hash estable) para poder cancelar la correcta.

En `MillenniumService.kt`, dentro del `companion object`:
```kotlin
    private const val APPROVAL_CHANNEL_ID = "millennium_fg"  // reuse FGS channel

    @JvmStatic
    fun postApprovalNotification(
        context: Context,
        sessionId: String,
        senderAlias: String,
        fileCount: Int,
        totalSize: Long,
    ) {
        val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val notifId = ("approval:" + sessionId).hashCode()

        fun action(approved: Boolean, label: String, reqBase: Int): NotificationCompat.Action {
            val intent = Intent(context, ApprovalReceiver::class.java).apply {
                this.action = ApprovalReceiver.ACTION_DECIDE
                putExtra(ApprovalReceiver.EXTRA_SESSION_ID, sessionId)
                putExtra(ApprovalReceiver.EXTRA_APPROVED, approved)
                putExtra(ApprovalReceiver.EXTRA_NOTIF_ID, notifId)
            }
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            // Distinct requestCode per (session, decision) so PendingIntents
            // don't collide/overwrite each other.
            val pi = PendingIntent.getBroadcast(
                context, notifId xor reqBase, intent, flags
            )
            return NotificationCompat.Action.Builder(0, label, pi).build()
        }

        val sizeMb = totalSize.toDouble() / (1024.0 * 1024.0)
        val notif = NotificationCompat.Builder(context, APPROVAL_CHANNEL_ID)
            .setContentTitle("Incoming files from $senderAlias")
            .setContentText(String.format("%d file(s) · %.1f MB", fileCount, sizeMb))
            .setSmallIcon(R.mipmap.ic_launcher)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setAutoCancel(true)
            .addAction(action(true,  "Accept",  0x1))
            .addAction(action(false, "Decline", 0x2))
            .build()
        nm.notify(notifId, notif)
    }

    @JvmStatic
    fun cancelApprovalNotification(context: Context, sessionId: String) {
        val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.cancel(("approval:" + sessionId).hashCode())
    }
```

Nuevo archivo `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/ApprovalReceiver.kt`:
```kotlin
package com.guidocameraeq.millennium

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class ApprovalReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_DECIDE = "com.guidocameraeq.millennium.APPROVAL_DECIDE"
        const val EXTRA_SESSION_ID = "session_id"
        const val EXTRA_APPROVED = "approved"
        const val EXTRA_NOTIF_ID = "notif_id"

        // Implemented in Rust (android_notify.rs). Resolves the oneshot.
        @JvmStatic
        external fun nativeResolveApproval(sessionId: String, approved: Boolean)
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_DECIDE) return
        val sessionId = intent.getStringExtra(EXTRA_SESSION_ID) ?: return
        val approved = intent.getBooleanExtra(EXTRA_APPROVED, false)
        val notifId = intent.getIntExtra(EXTRA_NOTIF_ID, 0)

        // Dismiss the notification immediately for snappy UX.
        (context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
            .cancel(notifId)

        // The native lib is already loaded in-process (Tauri loaded it at
        // Activity start). If the process was killed there's no oneshot to
        // resolve anyway, so a failed lookup is benign.
        try {
            nativeResolveApproval(sessionId, approved)
        } catch (t: Throwable) {
            // Library not loaded (process died) — nothing to resolve.
        }
    }
}
```

Registrá el receiver en el manifest, dentro de `<application>` (no exportado; solo lo dispara nuestro propio `PendingIntent`):
```xml
        <receiver
          android:name=".ApprovalReceiver"
          android:exported="false" />
```

**Por qué.** El `oneshot` de Rust es el punto de sincronización correcto y ya existe (`resolve_approval`). Solo faltaba una segunda ruta para llegar a él que no dependa del WebView. Un `BroadcastReceiver` + JNI es lo más liviano: no agrega un plugin Tauri custom, reutiliza el canal de notificación FGS y el `System.loadLibrary` que Tauri ya hizo. El nombre `Java_com_guidocameraeq_millennium_ApprovalReceiver_nativeResolveApproval` respeta la convención JNI (`Java_<pkg con _>_<Clase>_<método>`) que enlaza con la `external fun` de Kotlin.

**Cuidado con.**
- **Race WebView vs. nativa:** si el usuario decide en ambos lados, `resolve_approval` es idempotente por diseño — el segundo `remove` del registry devuelve `None` y retorna `false` sin efecto. No hay doble consumo del `oneshot`.
- **JNI method signature:** la firma `"(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;IJ)V"` debe coincidir EXACTO con los tipos Kotlin (`Context, String, String, Int, Long` → `void`). Un mismatch es un crash en runtime, no en compile.
- **`ndk_context::android_context()`** solo es válido después de que la NativeActivity inicializó el contexto; como la notificación se dispara desde un request HTTP entrante, el proceso ya está corriendo, así que es seguro.
- **ProGuard/R8 en release** (`build.gradle.kts:58-66`, `isMinifyEnabled = true`) puede eliminar o renombrar `ApprovalReceiver.nativeResolveApproval` y el receiver. Agregá reglas keep: `-keep class com.guidocameraeq.millennium.ApprovalReceiver { *; }` y `-keepclasseswithmembernames class * { native <methods>; }` en el `.pro` del módulo. Sin esto la aprobación nativa compila pero falla en el APK de release.
- **No borres** el `emit("incoming-files-request")` ni el modal WebView: son el fallback foreground y el camino de auto-accept sigue igual.

---

### Tarea A.4 — Pedir `POST_NOTIFICATIONS` antes de arrancar el servicio

**Problema.** En Android 13+ (`SDK_INT >= 33`) `POST_NOTIFICATIONS` es una runtime permission. El manifest ya la declara (`AndroidManifest.xml:11`) pero nunca se pide en runtime. Sin el grant, la notificación FGS (Tarea A.1/A.2) y la de aprobación (Tarea A.3) se postean pero **no se muestran**. `MainActivity.onCreate` arranca el servicio directo sin pedirla (`MainActivity.kt:13-20`).

**Archivo(s).** `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MainActivity.kt:8-22`

**Estado actual.**
```kotlin
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // Kick off the foreground service so discovery keeps working when
    // the activity is backgrounded / the screen turns off.
    val serviceIntent = Intent(this, MillenniumService::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      startForegroundService(serviceIntent)
    } else {
      startService(serviceIntent)
    }
  }
}
```

**Cambio.** Pedí `POST_NOTIFICATIONS` con `ActivityCompat.requestPermissions` antes de arrancar el servicio en API 33+; el `startService` real se hace en un helper que se llama tanto tras el grant como directamente en API < 33.
```kotlin
package com.guidocameraeq.millennium

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  companion object {
    private const val REQ_POST_NOTIFICATIONS = 1001
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)   // Tauri boots the Rust core here
    MillenniumService.coreAlive = true   // see Tarea A.2

    // Android 13+ needs POST_NOTIFICATIONS granted at runtime or the FGS
    // + approval notifications are silently suppressed. Ask first, then
    // start the service from onRequestPermissionsResult.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
            != PackageManager.PERMISSION_GRANTED) {
      ActivityCompat.requestPermissions(
        this,
        arrayOf(Manifest.permission.POST_NOTIFICATIONS),
        REQ_POST_NOTIFICATIONS
      )
    } else {
      startMillenniumService()
    }
  }

  override fun onRequestPermissionsResult(
    requestCode: Int,
    permissions: Array<out String>,
    grantResults: IntArray
  ) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == REQ_POST_NOTIFICATIONS) {
      // Start the service regardless of grant: the FGS itself is still
      // useful (multicast lock, process lifetime); only the *visible*
      // notification depends on the grant.
      startMillenniumService()
    }
  }

  private fun startMillenniumService() {
    val serviceIntent = Intent(this, MillenniumService::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      startForegroundService(serviceIntent)
    } else {
      startService(serviceIntent)
    }
  }
}
```

**Por qué.** Sin el grant, todo el trabajo de A.1-A.3 es invisible: la notificación FGS y la de aprobación no aparecen. Pedirla antes del `startService` garantiza que la primera notificación FGS ya tenga permiso.

**Cuidado con.** No bloquees el arranque del servicio si el usuario deniega — la FGS sigue extendiendo la vida del proceso y sosteniendo el `MulticastLock`; solo perdés la visibilidad. `startForegroundService` en Android 12+ debe llamar `startForeground()` dentro de ~5 s o el sistema lanza `ForegroundServiceDidNotStartInTimeAllowedException`; `MillenniumService.onCreate` ya llama `startForeground` sincrónicamente (`MillenniumService.kt:47`), así que el retraso del prompt no lo afecta (el service arranca DESPUÉS del grant). `TIRAMISU` es API 33.

---

### Tarea A.5 — Battery/Doze: exención + peers "reachable-on-demand"

**Problema.** Con Doze/App Standby, aunque la FGS mantenga el proceso, el OS puede diferir alarmas y restringir CPU/red en background, y muchos OEM (Xiaomi/MIUI, Huawei, Oppo, Samsung) matan servicios agresivamente. Además, del lado del protocolo, un phone que dejó de emitir discovery por Doze puede ser **descartado del peer list** por el peer de escritorio y volverse "no enviable", aunque en realidad respondería si le tocás.

**Archivo(s).** `src-tauri/gen/android/app/src/main/AndroidManifest.xml` (nueva permission) · `src-tauri/gen/android/app/src/main/java/com/guidocameraeq/millennium/MainActivity.kt` (prompt) · contexto de discovery en `src-tauri/src/discovery.rs` / `src-tauri/src/udp_discovery.rs` (expiración de peers)

**Estado actual.** No hay ninguna solicitud de exención de batería, y la app no maneja Doze. La lógica de expiración de peers vive en discovery (fuera del alcance de lectura de esta fase; ver nota en "Cuidado con").

**Cambio.**

1. Declará la permission en el manifest (junto a las otras `uses-permission`):
```xml
    <uses-permission android:name="android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS" />
```

2. Pedí la exención **una sola vez** (guardá un flag en `SharedPreferences` para no molestar en cada arranque). En `MainActivity`, tras arrancar el servicio, agregá:
```kotlin
  private fun maybeRequestBatteryExemption() {
    val prefs = getSharedPreferences("millennium_prefs", MODE_PRIVATE)
    if (prefs.getBoolean("asked_battery_exemption", false)) return

    val pm = getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
    if (!pm.isIgnoringBatteryOptimizations(packageName)) {
      try {
        val intent = Intent(
          android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
          android.net.Uri.parse("package:$packageName")
        )
        startActivity(intent)
      } catch (_: Exception) {
        // Some OEMs don't expose this intent; fall back to the generic
        // battery settings screen or just skip.
      }
    }
    prefs.edit().putBoolean("asked_battery_exemption", true).apply()
  }
```
Llamalo desde `startMillenniumService()` (o al final de `onCreate` una vez que el servicio arrancó). Requiere `import android.content.Context`.

3. **Protocolo — peers "reachable-on-demand".** Del lado de discovery, un peer que fue visto hace poco pero dejó de emitir NO debe eliminarse del peer list; debe marcarse como `quiet`/`reachable-on-demand` y seguir siendo un destino de envío válido (el intento de `/prepare-upload` lo despertará o fallará explícitamente, que es mejor UX que "desapareció"). Concretamente: introducí dos umbrales en la expiración de peers — un `QUIET_AFTER` (p. ej. sin heartbeat en > 30 s → estado `quiet`, sigue en la lista) y un `DROP_AFTER` mucho más largo (p. ej. > 10 min → recién ahí se elimina). La UI muestra los `quiet` con un indicador atenuado en vez de ocultarlos.

**Por qué.** Para una app sideloaded (no Play Store), pedir `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` es aceptable y es la única forma realista de que Android mantenga la recepción viva con la pantalla apagada por períodos largos. El modelo "reachable-on-demand" evita que Doze en el phone haga que el escritorio lo declare muerto y quite la opción de enviarle.

**Documentá pasos de whitelist por OEM** (para incluir en el onboarding/README, no en código): en MIUI/Xiaomi activar "Autostart" y fijar la batería en "Sin restricciones"; en Huawei/Honor añadir la app a "Protected apps" / "Managed manually"; en Samsung sacarla de "Sleeping apps"/"Deep sleeping apps" y desactivar "Put unused apps to sleep"; en Oppo/OnePlus/Realme (ColorOS) permitir "Allow background activity" y "Auto-launch". Estos ajustes no son programables; el usuario los hace a mano.

**Cuidado con.** `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` con `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` (el intent que apunta al `package:`) muestra un diálogo del sistema y es aceptable para sideload; **no** uses la variante que Google Play prohíbe salvo whitelisting — como esta app no va a Play, no hay problema de policy. La lógica de expiración de peers vive en `discovery.rs`/`udp_discovery.rs` (esta fase solo especifica el requisito; la implementación fina de los umbrales puede coordinarse con la fase de discovery del plan si existe). No cambies el intervalo de heartbeat sin revisar el consumo de batería.

## Cómo verificar

Build/run/deploy Android: ver `../00-SHARED-CONTEXT.md`. Con un peer de escritorio corriendo en la misma LAN:

1. **Tipo de FGS (A.1).** Tras instalar, `adb shell dumpsys activity services com.guidocameraeq.millennium | grep -i foreground` debe mostrar `fgsType=connectedDevice` (no `dataSync`). No debe aparecer ningún warning de "foreground service type not allowed" en `adb logcat`.
2. **Zombie (A.2).** Con la app en background, forzá la muerte del proceso: `adb shell am kill com.guidocameraeq.millennium`. Observá la barra de notificaciones: la notificación "Linked — receiving transfers in the background" **debe desaparecer** (con Opción B). Antes del fix, reaparecía sobre un proceso muerto. Confirmá con `adb shell ps -A | grep millennium` que no hay proceso, y que un `/prepare-upload` desde el escritorio falla en vez de colgar.
3. **Aprobación nativa (A.3).** Abrí la app, mandala a background (Home), apagá la pantalla. Desde el escritorio enviá archivos. Debe aparecer una notificación "Incoming files from <alias>" con botones **Accept** / **Decline**.
   - Tocar **Accept** → la transferencia completa; en `adb logcat` aparece `[android_notify] native decision session=... approved=true resolved=true`.
   - Tocar **Decline** → el escritorio recibe `403`/"rejected"; log con `approved=false`.
   - Ignorar 60 s → expira; aparece `incoming-files-timeout` y la notificación se retira (`cancel_approval_notification`).
   - Con la app en **foreground**, la misma acción sigue mostrando el modal WebView y no hay doble prompt.
4. **POST_NOTIFICATIONS (A.4).** En un device Android 13+ recién instalado, al primer arranque debe aparecer el diálogo de permiso de notificaciones **antes** de que se muestre la notificación FGS. Denegarlo no debe crashear; concederlo hace visible la notificación FGS.
5. **Batería (A.5).** Primer arranque: aparece una vez el diálogo "Allow Millennium to run in the background / ignore battery optimizations". En arranques siguientes no reaparece (flag `asked_battery_exemption`). `adb shell dumpsys deviceidle whitelist | grep millennium` lista el paquete tras conceder.
6. **Unit test (Rust).** Agregá en `http_server.rs` (módulo `#[cfg(test)] mod tests`) un test de idempotencia de `resolve_approval` para blindar el race WebView-vs-nativa:
```rust
#[test]
fn resolve_approval_is_idempotent() {
    use tokio::sync::oneshot;
    let (tx, _rx) = oneshot::channel::<bool>();
    approval_registry().lock().unwrap().insert("sess-x".into(), tx);
    assert!(resolve_approval("sess-x", true));   // first resolves
    assert!(!resolve_approval("sess-x", false)); // second is a no-op
}
```
Assertion esperada: la primera llamada devuelve `true`, la segunda `false` (el registry ya no tiene la entrada). Corré la suite Rust según `../00-SHARED-CONTEXT.md`.

## Riesgo y rollback

- **A.1 (tipo FGS)** es de bajo riesgo y **shippable independientemente**; rollback = volver a `dataSync` y la permission previa. Es un cambio de manifest puro.
- **A.2 (`START_NOT_STICKY` + `coreAlive`)** es el más delicado: si el flag `coreAlive` quedara `false` por error (p. ej. si se olvida setearlo en `MainActivity`), el servicio haría `stopSelf()` en cada arranque y Android nunca mostraría la FGS ni sostendría el MulticastLock. Mitigá verificando el punto 2 de "Cómo verificar" (la notificación DEBE seguir apareciendo mientras la app está abierta). Rollback = volver a `START_STICKY` y quitar el early-return de `onCreate`.
- **A.3 (aprobación nativa)** agrega dependencias (`jni`, `ndk-context`) y una superficie JNI nueva; el riesgo es un mismatch de firma o R8 eliminando el símbolo native en release. Es **shippable de forma independiente** de A.5. Rollback = quitar la rama `#[cfg(target_os = "android")]` en `handle_prepare_upload`, el módulo `android_notify`, `ApprovalReceiver.kt` y las deps; el flujo vuelve al modal WebView-only (Android inutilizable en background, como antes). Testeá específicamente el APK de **release** (no solo debug) por el tema ProGuard.
- **A.4 (POST_NOTIFICATIONS)** es de bajo riesgo; rollback = revertir `MainActivity`. No romper: el `startService` debe seguir ocurriendo aunque se deniegue el permiso.
- **A.5 (batería/peers)** el prompt de exención es cosmético/best-effort y reversible (borrar el flag en `SharedPreferences`); el cambio de umbrales de expiración de peers toca `discovery.rs`/`udp_discovery.rs` y debe coordinarse con la fase de discovery para no romper la lógica de online/offline del escritorio. Shippealo por separado de A.1-A.4.
- **Orden de merge seguro:** A.1 → A.4 → A.2 → A.3 → A.5. A.1 y A.4 son inocuos y desbloquean visibilidad; A.2 y A.3 se testean juntos porque A.3 depende de que la app siga viva en background (que A.2 no rompa). A.5 al final.
