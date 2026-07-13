# Spec de remediación — Android

> Requiere haber leído `../00-SHARED-CONTEXT.md`. **Antes de tocar código, resolvé la decisión estratégica de abajo.**

## Meta del spec

Llevar Android de **"nunca funcionó bien"** a un peer confiable de la red. El **protocolo y el core Rust son portables y se conservan**; lo que hay que rediseñar es la **capa de entrega en el teléfono**, cuyas tres suposiciones son falsas en Android:

1. *"El WebView siempre está disponible para manejar la UX"* → está **congelado** en segundo plano ⇒ las aprobaciones y notificaciones deben ser **nativas**.
2. *"El proceso sobrevive a la Activity porque hay un foreground service"* → un FGS `dataSync` tiene **tope de ~6 h** en Android 15+ y `START_STICKY` revive un **shell vacío** sin runtime Rust.
3. *"IP de ruta por defecto == IP de la LAN"* → falso cuando hay datos móviles ⇒ el descubrimiento debe **bindear a la interfaz Wi-Fi** explícitamente.

## ⚑ Decisión estratégica (resolver con el humano antes de empezar)

El punto (2) tiene dos caminos, y define cuánto esfuerzo es Android:

- **Opción A — Android de primera clase (núcleo headless).** El foreground service **carga la lib nativa y corre el servidor + descubrimiento de Rust por sí mismo**, independiente de la Activity (que pasa a ser un visor liviano). El teléfono es un peer confiable incluso con la pantalla apagada. **Más trabajo** (~1-2 semanas), pero es la única forma de que "recibir en el bolsillo" funcione de verdad.
- **Opción B — Foreground-only honesto.** La app **declara** que solo funciona con la pantalla encendida y en primer plano: el servicio retorna `START_NOT_STICKY` y `stopSelf()` cuando no hay runtime, la notificación desaparece honestamente, y la UX comunica "abrí la app para recibir". **Mucho menos trabajo**; suficiente si el uso real es "las dos manos en los dos dispositivos".

> **Recomendación:** si el caso de uso es mandarle algo al teléfono mientras está guardado, hace falta la **Opción A**. Si en la práctica siempre tenés la app abierta cuando transferís, la **Opción B** es honesta y barata. Los detalles de implementación de ambas están en la Fase A (Tarea A.2). **Elegí una antes de codear.**

## Prerrequisitos desde el spec de Windows

Estas correcciones viven en Rust compartido y **también arreglan Android**. Hacelas (o coordiná con) el spec de Windows:

- **Windows Fase 1 (`../windows/phase-1-discovery.md`):** sacar el filtro `/24` y aplicar la corrección de IP del UDP. Sin esto, el descubrimiento de Android sigue roto aun con la interfaz Wi-Fi bien elegida. La Fase B de Android agrega **encima** el binding a Wi-Fi.
- **Windows Fase 3 (`../windows/phase-3-security.md`):** el pinning real de certificado. Es lo que hace que confiar en un teléfono como peer sea seguro.

## Fases

| Fase | Archivo | Qué resuelve | Esfuerzo | Riesgo |
|---|---|---|---|---|
| **A** | [`phase-A-lifecycle-and-approval.md`](phase-A-lifecycle-and-approval.md) | Tipo de servicio + `onTimeout`, zombi `START_STICKY`, **aprobación de archivos por notificación nativa**, `POST_NOTIFICATIONS`, batería/Doze | Alto | Alto |
| **B** | [`phase-B-discovery-and-storage.md`](phase-B-discovery-and-storage.md) | Bindear a la interfaz Wi-Fi para descubrir, **recibir por streaming a MediaStore** (fin del OOM), SAF sin colisiones, network security config | Medio | Medio |
| **C** | [`phase-C-clipboard-qr-mobile.md`](phase-C-clipboard-qr-mobile.md) | Portapapeles dentro de lo que el SO permite (leer al foco / escribir al recibir), escaneo QR (3 defectos), **CSS mobile-first** desde cero, fuentes locales | Medio | Bajo-Medio |

Orden recomendado: **A → B → C**. La Fase A es la de mayor impacto (sin ella, recibir en segundo plano nunca anda). La Fase C es la más independiente y se puede hacer en paralelo.

## Cómo probar en Android

- Necesitás un **teléfono real** (el emulador no reproduce Doze, multicast, ni el comportamiento de segundo plano de forma fiel).
- Build de debug para iterar: `npm run tauri android dev` (con el teléfono conectado por USB y depuración activada).
- Para logs nativos: `adb logcat` filtrando por el package `com.guidocameraeq.millennium` y por el tag de Rust (`RustStdoutStderr` / el que use Tauri).
- Casos de prueba mínimos por fase están en la sección "Cómo verificar" de cada archivo.

## Criterios de aceptación del spec completo (Definition of Done)

- [ ] **Descubrimiento:** con Wi-Fi + datos móviles ambos encendidos y la app en primer plano, el teléfono ve a los peers de la LAN y ellos lo ven, de forma estable (sin parpadeo).
- [ ] **Recepción:** enviar un archivo desde la PC al teléfono muestra un prompt de aceptar/rechazar (nativo si la app está en segundo plano, según la opción elegida) y el archivo aparece en `/Downloads`.
- [ ] **Archivos grandes:** recibir un video de varios GB no mata el proceso (sin OOM) y aparece completo en Downloads.
- [ ] **Ciclo de vida (según opción):** Opción A → el teléfono sigue siendo alcanzable con la pantalla apagada un tiempo razonable; Opción B → la notificación refleja honestamente que solo anda en primer plano.
- [ ] **Portapapeles:** al menos las direcciones permitidas por el SO funcionan (leer al enfocar la app, escribir al recibir).
- [ ] **QR:** el escaneo con cámara empareja de verdad (los 3 defectos corregidos).
- [ ] **UI móvil:** el compositor de texto no se colapsa, los modales respetan las safe areas, y los botones de aceptar/rechazar no quedan bajo la barra de gestos.
- [ ] Build release firmado (`npm run tauri android build --apk`) instala y arranca.
