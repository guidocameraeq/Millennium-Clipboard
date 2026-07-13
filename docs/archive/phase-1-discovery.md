> ✅ **IMPLEMENTADA 2026-07-13** — todas las Tareas (1.1–1.7) aplicadas + review adversarial. Verificación física con 2 dispositivos: PENDIENTE del usuario. Ver `docs/CHANGELOG.md` y `docs/SESSION_HANDOFF.md`. Archivado.

# Consolidar el descubrimiento (fin del parpadeo) — Fase 1: Discovery

> Parte del plan de remediación de Millennium Clipboard. Leé primero `../00-SHARED-CONTEXT.md`.
> **Plataforma:** Rust compartido (arregla también el discovery en foreground de Android) · **Prerrequisitos:** ninguno · **Esfuerzo:** ~1–1.5 días · **Riesgo:** med

## Objetivo
Unificar las tres fuentes de descubrimiento (mDNS, UDP broadcast, TCP probe) bajo **una sola política de reconciliación** para el `PeerMap`, de modo que dejen de pisarse la IP/puerto entre sí. Eliminar el sondeo TCP incondicional cada 6 s y las suposiciones de `/24` cableadas, reemplazándolos por un *reaper* barato basado en `last_seen` y un probe bajo demanda. El resultado observable es que un peer estable deja de parpadear entre online/offline y el CPU en reposo baja drásticamente.

## Definición de "hecho"
- [ ] Existe **una** función/politica que decide cómo se actualiza un `PeerRecord` existente; mDNS ya no sobrescribe incondicionalmente `existing.ip`.
- [ ] En `udp_discovery::handle_packet`, la rama de peer existente actualiza `existing.ip = src_ip` y `existing.port = pkt.tcp_port` cuando difieren, y emite `peers-changed`.
- [ ] `PeerRecord` tiene un campo que distingue el origen del dato (p. ej. `confirmed: bool` + `confirmed_at`/`last_seen`) para que el probe pueda marcar una IP como confirmada.
- [ ] No queda ningún `daemon.browse(SERVICE_TYPE)` dentro del loop del poller; el browse sucede una sola vez al arrancar (más `rebrowse()` bajo demanda del usuario).
- [ ] No existe el `retain()` que borra peers por `/24`, ni la función `subnet_prefix_24`; `derive_subnet_broadcast` ya no asume `/24`.
- [ ] Un peer que se ve por UDP pero al que **nunca** se le hace probe TCP sigue apareciendo online mientras su `last_seen` sea reciente, y pasa a offline solo cuando `last_seen` supera ~3× el intervalo UDP.
- [ ] El contador de fallos ya no puede desbordar el `u8`; los peers que salen del candidate set se borran del mapa `failures`.
- [ ] Todos los `tokio::time::interval` del proyecto llaman a `set_missed_tick_behavior(MissedTickBehavior::Skip)`.
- [ ] `compute_local_ip()` prefiere una interfaz de rango privado no-virtual y se recalcula ante cambios de red.
- [ ] El formato de wire UDP (`DiscoveryPacket`) queda **idéntico** al actual (compat con peers viejos).
- [ ] `cargo build` y `cargo clippy` limpios (ver `00-SHARED-CONTEXT.md`).

## Tareas

### Tarea 1.1 — Política única de reconciliación del `PeerMap`
**Problema.** Tres fuentes escriben en el mismo `PeerMap` sin coordinación. mDNS sobrescribe `existing.ip` con la IP del A-record (a menudo la de una NIC virtual del peer remoto), mientras que el datagrama UDP —que trae la IP real del socket de origen— la ignora explícitamente y solo loguea el desacuerdo. El poller TCP después sondea la IP equivocada, falla, y el peer parpadea. Hace falta una prioridad: **una IP confirmada por probe manda; la IP de origen del datagrama UDP le gana a cualquier IP anunciada; mDNS solo puede INSERTAR peers desconocidos, nunca sobrescribir una IP/puerto confirmada.**

**Archivo(s).** `src-tauri/src/discovery.rs:52` (struct `PeerRecord`), `src-tauri/src/discovery.rs:640` (rama existente de `handle_event`), `src-tauri/src/udp_discovery.rs:245` (rama existente de `handle_packet`).

**Estado actual.** `PeerRecord` no distingue origen:
```rust
#[derive(Debug, Clone)]
pub struct PeerRecord {
    pub id: String,
    pub name: String,
    pub hex_id: String,
    pub ip: String,
    pub port: u16,
    pub icon_type: String,
    #[allow(dead_code)]
    pub last_seen: Instant,
}
```
En `discovery.rs` la rama existente pisa la IP sin condición:
```rust
Some(existing) => {
    let same = existing.name == alias
        && existing.hex_id == hex_id
        && existing.ip == ip
        && existing.port == port
        && existing.icon_type == icon_type;
    existing.last_seen = Instant::now();
    if !same {
        crate::runtime_log::info(format!(
            "[mdns] resolve {} '{}' changed: ip {}->{} port {}->{} (announced addrs: {:?})",
            fp_short, alias, existing.ip, ip, existing.port, port, all_addrs
        ));
        existing.name = alias;
        existing.hex_id = hex_id;
        existing.ip = ip;
        existing.port = port;
        existing.icon_type = icon_type;
    }
    !same
}
```
En `udp_discovery.rs` la rama existente ve el desacuerdo pero **no corrige**:
```rust
Some(existing) => {
    existing.last_seen = Instant::now();
    if existing.name != pkt.alias {
        existing.name = pkt.alias.clone();
    }
    if existing.ip != src_ip {
        crate::runtime_log::warn(format!(
            "[udp] IP DISAGREEMENT for {}: stored={} datagram_src={} (UDP currently ignores the correction — TCP probe will fail against stored IP)",
            fp_short, existing.ip, src_ip
        ));
    }
    false
}
```

**Cambio.**
1. Añadir a `PeerRecord` un campo de confirmación. Mínimo:
```rust
#[derive(Debug, Clone)]
pub struct PeerRecord {
    pub id: String,
    pub name: String,
    pub hex_id: String,
    pub ip: String,
    pub port: u16,
    pub icon_type: String,
    pub last_seen: Instant,
    /// La IP/puerto fueron confirmados por un probe TCP (o por la IP de
    /// origen de un datagrama UDP). Una vez `true`, mDNS ya no los pisa.
    pub confirmed: bool,
}
```
   (quitar el `#[allow(dead_code)]` de `last_seen`, que ahora se usa en la Tarea 1.4). Actualizar **todos** los sitios que construyen `PeerRecord` para poner `confirmed`: en `discovery.rs` el probe TCP (línea ~415) pone `confirmed: true`; el insert de mDNS (línea ~668) pone `confirmed: false`; en `udp_discovery.rs` el insert (línea ~267) pone `confirmed: true` (la IP viene del socket de origen, es real).

2. En `discovery.rs handle_event`, rama `Some(existing)`: **solo actualizar `ip`/`port` si el record no está `confirmed`.** Nombre/hex/icon del A-record TXT sí se pueden refrescar siempre (son metadata, no ruteo). Reescribir así:
```rust
Some(existing) => {
    existing.last_seen = Instant::now();
    let meta_changed = existing.name != alias
        || existing.hex_id != hex_id
        || existing.icon_type != icon_type;
    // mDNS NUNCA pisa una IP/puerto ya confirmados por probe/UDP.
    let route_changed = if !existing.confirmed
        && (existing.ip != ip || existing.port != port)
    {
        crate::runtime_log::info(format!(
            "[mdns] resolve {} '{}' route (unconfirmed) {}:{} -> {}:{}",
            fp_short, alias, existing.ip, existing.port, ip, port
        ));
        existing.ip = ip;
        existing.port = port;
        true
    } else {
        if existing.confirmed && (existing.ip != ip || existing.port != port) {
            crate::runtime_log::info(format!(
                "[mdns] ignoring A-record {}:{} for confirmed peer {} (keeping {}:{})",
                ip, port, fp_short, existing.ip, existing.port
            ));
        }
        false
    };
    if meta_changed {
        existing.name = alias;
        existing.hex_id = hex_id;
        existing.icon_type = icon_type;
    }
    meta_changed || route_changed
}
```
   (Ojo: las variables `ip`/`port` se consumen por `existing.ip = ip`. Si el `else` necesita loguear `ip`/`port` después del `if`, clonar `ip` antes o mover el log. Lo más simple: capturar `let (a_ip, a_port) = (ip.clone(), port);` al inicio de la rama y loguear con esas copias.)

3. En `udp_discovery.rs handle_packet`, rama `Some(existing)`: aplicar la corrección de IP/puerto y emitir. La IP de origen del datagrama es autoritativa y **le gana a cualquier IP anunciada** (esté o no `confirmed`):
```rust
Some(existing) => {
    existing.last_seen = Instant::now();
    existing.confirmed = true; // la src IP del datagrama es real
    let mut route_changed = false;
    if existing.ip != src_ip {
        crate::runtime_log::info(format!(
            "[udp] correcting IP for {}: {} -> {} (datagram src wins)",
            fp_short, existing.ip, src_ip
        ));
        existing.ip = src_ip.clone();
        route_changed = true;
    }
    if existing.port != pkt.tcp_port {
        existing.port = pkt.tcp_port;
        route_changed = true;
    }
    if existing.name != pkt.alias {
        existing.name = pkt.alias.clone();
        route_changed = true;
    }
    route_changed
}
```
   Cambiar el binding externo `let was_new = { ... }` a algo como `let should_emit = { ... }` que sea `true` tanto en el `None` (peer nuevo) como cuando `route_changed` en la rama existente, y usar ese flag para emitir `peers-changed` (ver bloque `if was_new { ... }` en `udp_discovery.rs:284`, que hoy solo emite en peer nuevo).

**Por qué.** Este es el root cause del parpadeo asimétrico documentado en el propio comentario `IP DISAGREEMENT`. La IP de origen del socket UDP no puede mentir (es la que el kernel vio llegar); la IP del A-record de mDNS sí (el peer remoto anuncia todas sus NICs, incluidas WSL/Hyper-V). Con una sola política, el probe TCP siempre apunta a una IP alcanzable.

**Cuidado con.** No romper el caso de un peer que legítimamente cambia de IP (roaming Wi-Fi): por eso UDP —que trae la IP fresca real— siempre corrige, y el reaper de la Tarea 1.4 baja `confirmed` implícitamente al expirar `last_seen`. No toques el `WirePeer`/`to_wire` (el campo `confirmed` es interno, no va al frontend). Mantené el guard `id == my_fingerprint` intacto en ambos archivos.

### Tarea 1.2 — Quitar el `browse()` por tick (poller y `rebrowse`)
**Problema.** El poller hace `daemon.browse(SERVICE_TYPE)` en cada iteración (cada 6 s). En `mdns-sd`, llamar `browse()` repetidamente sobre el mismo servicio puede reemplazar/duplicar el listener y, en algunos casos, silenciar el receiver original. Alcanza con hacer browse una vez al arrancar; `mdns-sd` sigue recibiendo anuncios pasivamente.

**Archivo(s).** `src-tauri/src/discovery.rs:338-339` (dentro del loop del poller) y `src-tauri/src/discovery.rs:511` (`rebrowse`).

**Estado actual.**
```rust
loop {
    tick.tick().await;

    // Still poke mDNS so any newcomer that's announcing is heard.
    let _ = daemon_for_poll.browse(SERVICE_TYPE);
```
```rust
pub fn rebrowse(state: &DiscoveryState) -> Result<(), mdns_sd::Error> {
    state.daemon.browse(SERVICE_TYPE).map(|_| ())
}
```

**Cambio.**
1. Borrar las dos líneas del `browse()` por tick dentro del loop del poller (líneas 338–339). El `let daemon_for_poll = daemon.clone();` (línea 322) queda sin uso: eliminá también ese clone y su binding, salvo que la Tarea 1.4 lo necesite (no lo necesita). Si `daemon_for_poll` queda sin usar, `cargo` avisará; removelo.
2. **Mantener** `rebrowse()` como está: es la ruta *bajo demanda* que dispara el comando `rescan_peers` (`lib.rs:94`). Un solo browse extra iniciado por el usuario es seguro y deseado; el problema era hacerlo 10 veces por minuto sin que nadie lo pida.

**Por qué.** El browse inicial en `start()` (`let receiver = daemon.browse(SERVICE_TYPE)?;`, línea 263) ya deja el listener activo. Rebrowsear en cada tick no aporta descubrimiento nuevo (los anuncios llegan solos) y arriesga matar el listener, además de gastar CPU/red.

**Cuidado con.** No borres el browse de la línea 263 (ese es el único que crea el `receiver` que consume la task de `handle_event`). No cambies la firma de `rebrowse` ni su uso en `lib.rs`.

### Tarea 1.3 — Eliminar el gate `/24` cableado y su suposición en el broadcast
**Problema.** El poller descarta candidatos cuya IP no esté en el mismo `/24` que la propia, y **los borra del cache** (`peers_for_poll...remove(fp)`). Esto es incorrecto: la máscara real puede ser `/16`, `/23`, etc., y la alcanzabilidad la mide el probe, no una heurística de octetos. Además `derive_subnet_broadcast` asume `/24` para el broadcast dirigido.

**Archivo(s).** `src-tauri/src/discovery.rs:362-383` (el `retain` por `/24`), `src-tauri/src/discovery.rs:527-540` (`subnet_prefix_24`), `src-tauri/src/udp_discovery.rs:203-218` (`derive_subnet_broadcast`).

**Estado actual.**
```rust
let my_prefix = subnet_prefix_24(&my_ip_poll);
by_fp.retain(|fp, (ip, _, _, _)| {
    if let (Some(mine), Some(theirs)) = (my_prefix.as_ref(), subnet_prefix_24(ip)) {
        if mine != &theirs {
            crate::runtime_log::info(format!(
                "[poll] skipping {} @ {} — different /24 from {} (unreachable)",
                &fp[..16.min(fp.len())], ip, my_ip_poll
            ));
            peers_for_poll.lock().unwrap().remove(fp);
            return false;
        }
    }
    true
});
```
```rust
fn derive_subnet_broadcast(local_ip: &str) -> Option<SocketAddr> {
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 { return None; }
    // Assume /24 — by far the most common consumer LAN.
    Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(
            parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?, 255,
        )),
        UDP_DISCOVERY_PORT,
    ))
}
```

**Cambio.**
1. Borrar por completo el bloque `let my_prefix = ...; by_fp.retain(...)` (líneas 362–383) del poller. Los candidatos inalcanzables se manejan solos vía fallos de probe (Tarea 1.5) y el reaper (Tarea 1.4).
2. Borrar la función `subnet_prefix_24` (líneas 527–540) y el binding `my_ip_poll` si queda sin uso tras el borrado (verificá: se usaba solo en el log y el `retain`; si nada más lo referencia, quitar `let my_ip_poll = identity.local_ip.clone();` en línea ~324).
3. En `udp_discovery.rs`, mantener el broadcast global a `255.255.255.255` (ya existe y funciona en toda LAN de consumo). Para el broadcast dirigido, **no asumir `/24`**: la opción más simple y correcta es *eliminar* `derive_subnet_broadcast` y quedarse solo con el broadcast global `255.255.255.255`, que ya cubre el segmento local. Si preferís conservar un broadcast dirigido, derivá la máscara real de la interfaz (ver nota abajo) en vez de hardcodear `.255`. Recomendación: **borrar `derive_subnet_broadcast`** y su uso (líneas 127, 155–159), dejando solo `socket.send_to(&bytes, broadcast)`.

**Por qué.** El gate `/24` produce falsos "unreachable" en cualquier LAN con máscara distinta de `/24` (oficinas `/16`, algunos routers `/23`) y borra peers válidos, causando parpadeo. `255.255.255.255` es *limited broadcast*: no se rutea fuera del enlace local pero llega a todos los hosts del segmento sin necesidad de conocer la máscara.

**Cuidado con.** No toques el `SocketAddr` de `broadcast` (255.255.255.255) ni `set_broadcast(true)`. El wire format del `DiscoveryPacket` no cambia. Si conservás el broadcast dirigido, no rompas el `if let Some(sb) = subnet_broadcast` (dejá el global siempre activo).

> Nota si querés la máscara real (opcional, no requerido para esta fase): la interfaz elegida por la Tarea 1.7 puede exponer prefix/netmask vía el crate `if-addrs` (`if_addrs::get_if_addrs()` → `Interface { addr: IfAddr::V4(Ifv4Addr { ip, netmask, broadcast }) }`, donde `broadcast` ya es la dirección de broadcast del segmento). Eso reemplazaría a `derive_subnet_broadcast` sin adivinar octetos.

### Tarea 1.4 — Reaper por `last_seen` + probe bajo demanda / backoff
**Problema.** Hoy el poller hace un probe TCP a `/info` de **todos** los peers cada 6 s, aunque el peer se esté viendo perfectamente por UDP (cada 5 s). Eso es CPU/red desperdiciada y la fuente principal del gasto en reposo. Un peer visto por UDP no necesita probe TCP para saberse vivo: alcanza con marcarlo offline cuando su `last_seen` expire.

**Archivo(s).** `src-tauri/src/discovery.rs:326-506` (toda la task del poller) y `src-tauri/src/udp_discovery.rs:249` / `:276` (ambos setean `last_seen`, que alimenta al reaper).

**Estado actual.** El poller sondea incondicionalmente cada 6 s (extracto):
```rust
let mut tick = tokio::time::interval(Duration::from_secs(6));
tick.tick().await; // skip the immediate first tick
loop {
    tick.tick().await;
    let _ = daemon_for_poll.browse(SERVICE_TYPE);
    // ... construye by_fp con TODOS los peers ...
    let probes: Vec<_> = by_fp.into_iter().map(...fetch_info...).collect();
    let results = join_all(probes).await;
    // ... inserta/borra según resultado ...
}
```
`last_seen` se setea en UDP pero **nunca se lee** para expirar peers (`PeerRecord.last_seen` está marcado `#[allow(dead_code)]`).

**Cambio.** Reestructurar la task del poller en **dos responsabilidades separadas**, ambas con `MissedTickBehavior::Skip` (Tarea 1.6):

**(A) Reaper barato — corre siempre, no hace red.** Cada ~2 s recorre el `PeerMap`, y para cada peer cuyo `last_seen.elapsed()` supere `PEER_TTL` (≈ 3× el intervalo UDP = `3 * BROADCAST_INTERVAL_SECS` = 15 s) lo remueve del mapa y emite `peers-changed`. Algoritmo:
```rust
const UDP_INTERVAL_SECS: u64 = 5; // debe coincidir con udp_discovery::BROADCAST_INTERVAL_SECS
let peer_ttl = Duration::from_secs(UDP_INTERVAL_SECS * 3); // ~15s

let mut reap = tokio::time::interval(Duration::from_secs(2));
reap.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
reap.tick().await;
loop {
    reap.tick().await;
    let mut changed = false;
    {
        let mut p = peers_for_poll.lock().unwrap();
        let before = p.len();
        p.retain(|_fp, rec| rec.last_seen.elapsed() < peer_ttl);
        changed = p.len() != before;
    }
    if changed {
        // limpiar fullnames huérfanos + emitir (reusar el bloque existente)
        let snapshot = build_wire_list(&peers_for_poll, &prefs_for_poll, &manual_for_poll,
            &aliases_for_poll, &clipboard_for_poll, &icons_for_poll);
        let _ = app_for_poll.emit("peers-changed", &snapshot);
    }
}
```
Exponer `UDP_INTERVAL_SECS` importando `udp_discovery::BROADCAST_INTERVAL_SECS` (hoy es `const` privado en `udp_discovery.rs:31` — hacelo `pub const` y referencialo, para que no se desincronicen).

**(B) Probe TCP bajo demanda / con backoff — no en cada tick.** El probe deja de ser un barrido periódico de todos. Se dispara:
   - **On demand:** antes de un envío (ya hay `fetch_info` en el path de send, no dupliques) y en `rescan_peers`/`rebrowse` (el usuario pidió refrescar).
   - **Backoff exponencial solo para peers NO oídos por UDP:** un candidato de `manual`/`favorites` que no aparece en el `PeerMap` (nunca llegó su UDP) se sondea con intervalo creciente: 6 s, 12 s, 24 s… hasta un tope (p. ej. 5 min). Mantené un `Map<String, (next_probe: Instant, backoff: Duration)>`. En cada tick del scheduler de probe (p. ej. cada 2 s) solo sondeás los candidatos cuyo `next_probe <= now`. Un peer que responde con `confirmed_at` fresco se inserta con `confirmed: true` y sale del set de backoff; uno que falla duplica su `backoff`.

   Representative sketch del scheduler de probe:
```rust
let mut probe_at: Map<String, Instant> = Map::new();
let mut backoff: Map<String, Duration> = Map::new();
let min_backoff = Duration::from_secs(6);
let max_backoff = Duration::from_secs(300);

let mut sched = tokio::time::interval(Duration::from_secs(2));
sched.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
sched.tick().await;
loop {
    sched.tick().await;
    let now = Instant::now();

    // candidatos = manual ∪ favorites que NO están vivos por UDP en el PeerMap
    let live: HashSet<String> = peers_for_poll.lock().unwrap().keys().cloned().collect();
    let mut due: Vec<(String, String, u16, String, String)> = Vec::new();
    for m in manual_for_poll.snapshot() {
        if m.fingerprint == my_fp_poll || live.contains(&m.fingerprint) { continue; }
        let at = probe_at.entry(m.fingerprint.clone()).or_insert(now);
        if *at <= now { due.push((m.fingerprint, m.ip, m.port, m.hex_id, m.icon_type)); }
    }
    for f in prefs_for_poll.favorites_snapshot() {
        if f.fingerprint == my_fp_poll || live.contains(&f.fingerprint) { continue; }
        let at = probe_at.entry(f.fingerprint.clone()).or_insert(now);
        if *at <= now { due.push((f.fingerprint, f.last_ip, f.last_port, f.hex_id, f.icon_type)); }
    }
    if due.is_empty() { continue; }

    let probes: Vec<_> = due.into_iter().map(|(fp, ip, port, hex, icon)| async move {
        let res = tokio::time::timeout(Duration::from_secs(5),
            crate::http_client::fetch_info(&ip, port)).await;
        (fp, ip, port, hex, icon, res)
    }).collect();
    let results = join_all(probes).await;

    let mut changed = false;
    for (fp, ip, port, hex, icon, res) in results {
        match res {
            Ok(Ok(info)) if info.fingerprint == fp => {
                backoff.remove(&fp);
                probe_at.remove(&fp);
                let rec = PeerRecord { id: fp.clone(), name: info.alias, hex_id: hex,
                    ip, port, icon_type: icon, last_seen: Instant::now(), confirmed: true };
                if peers_for_poll.lock().unwrap().insert(fp, rec).is_none() { changed = true; }
            }
            _ => {
                let b = backoff.entry(fp.clone()).or_insert(min_backoff);
                *b = (*b * 2).min(max_backoff);
                probe_at.insert(fp, now + *b);
            }
        }
    }
    if changed { /* emitir peers-changed */ }
}
```
   Podés implementar reaper (A) y scheduler (B) como **dos `tauri::async_runtime::spawn` separados**, o fusionarlos en un solo loop con dos `interval` en un `tokio::select!`. Preferí dos tasks: es más simple de razonar y cada una tiene su propio `MissedTickBehavior::Skip`.

**Por qué.** Elimina el barrido TCP de N peers cada 6 s (el gasto principal). Los peers activos se confirman gratis por UDP; solo los ausentes (manual/favorite sin UDP) pagan probe, y con backoff creciente. Esto además arregla Android: en foreground el UDP fluye, así que el reaper mantiene la lista viva sin martillar la red/batería.

**Cuidado con.** Mantené la detección de **DRIFT** (fingerprint distinto en la misma IP:port) del código actual (líneas 437–449): cuando el probe on-demand recibe un `info.fingerprint != fp`, hay que remover el peer viejo. No pierdas la limpieza de `fullnames` huérfanos (líneas 487–493) — reubicala en el reaper (A). El `peer_ttl` debe ser **estrictamente mayor** que el intervalo UDP para no matar peers entre dos hellos; 3× da margen a un hello perdido.

### Tarea 1.5 — Arreglar el overflow del contador `u8` de fallos
**Problema.** `failures: Map<String, u8>` incrementa con `*count += 1` sin saturación. Un peer que falla >255 veces desborda (panic en debug, wrap a 0 en release), y peor: peers que dejan el candidate set nunca se borran del mapa, que crece sin límite.

**Archivo(s).** `src-tauri/src/discovery.rs:331` (declaración), `:451`, `:457-458`, `:468`, `:474-475` (incrementos y chequeo de umbral).

**Estado actual.**
```rust
let mut failures: Map<String, u8> = Map::new();
// ...
Ok(Err(e)) => {
    let count = failures.entry(fp.clone()).or_insert(0);
    *count += 1;
    // ...
    if *count >= 3 && peers_for_poll.lock().unwrap().remove(&fp).is_some() { ... }
}
Err(_) => {
    let count = failures.entry(fp.clone()).or_insert(0);
    *count += 1;
    // ...
    if *count >= 3 && peers_for_poll.lock().unwrap().remove(&fp).is_some() { ... }
}
```

**Cambio.** En el rediseño de la Tarea 1.4 el conteo de fallos se reemplaza por el `backoff` map, que ya no puede desbordar (es un `Duration` con `.min(max_backoff)`). Si por lo que sea conservás un contador `u8`:
1. Usar `*count = count.saturating_add(1);` en vez de `*count += 1;` (ambos sitios).
2. Al remover un peer del `PeerMap`, remover también su entrada de `failures`/`backoff`/`probe_at` (`failures.remove(&fp);`).
3. Al inicio de cada barrido, purgar del mapa `failures`/`backoff` las claves que ya no están en el candidate set:
```rust
let candidates: HashSet<String> = /* fps actuales */;
failures.retain(|fp, _| candidates.contains(fp));
```

**Por qué.** Evita el panic/wrap y la fuga de memoria de un `Map` que solo crece. La saturación mantiene la semántica (>= 3 sigue disparando el drop) sin overflow.

**Cuidado con.** No bajes el umbral de 3: dos fallos aislados por packet loss no deben tumbar un peer. Con el rediseño de 1.4, el "drop tras 3 fallos" lo cubre el reaper por `last_seen` para peers UDP y el backoff para peers probe-only; asegurate de no dejar dos mecanismos de drop compitiendo sobre el mismo peer.

### Tarea 1.6 — `set_missed_tick_behavior(MissedTickBehavior::Skip)` en todos los intervalos
**Problema.** El comportamiento por defecto de `tokio::time::interval` es `Burst`: si una iteración tarda más que el período (p. ej. `join_all` de probes con timeout de 5 s dentro de un tick de 6 s), tokio dispara *ticks acumulados de golpe* al volver, generando ráfagas de trabajo y CPU. Ninguno de los tres intervalos del proyecto lo corrige.

**Archivo(s).** `src-tauri/src/discovery.rs:332`, `src-tauri/src/udp_discovery.rs:134`, `src-tauri/src/lib.rs:728`.

**Estado actual.**
```rust
// discovery.rs
let mut tick = tokio::time::interval(Duration::from_secs(6));
// udp_discovery.rs
let mut tick = tokio::time::interval(std::time::Duration::from_secs(BROADCAST_INTERVAL_SECS));
// lib.rs (clipboard poller)
let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
```

**Cambio.** Inmediatamente después de crear cada `interval`, añadir:
```rust
tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
```
Aplicarlo a **cada** `interval` del proyecto, incluidos los nuevos `reap`/`sched` de la Tarea 1.4 (ya incluido en sus sketches). Verificá con un grep final de `tokio::time::interval(` que ningún sitio quede sin el `set_missed_tick_behavior`.

**Por qué.** `Skip` descarta los ticks perdidos y agenda el próximo desde "ahora", evitando la ráfaga de catch-up. Es la causa directa de picos de CPU cuando un probe lento retrasa el poller. El clipboard poller (500 ms) es el más sensible: sin `Skip`, un `spawn_blocking` lento de `arboard` puede encolar decenas de lecturas.

**Cuidado con.** `Skip` es lo correcto para trabajo periódico idempotente (poll, broadcast). No lo confundas con `Delay` (que preserva la cadencia relativa) — para estos loops `Skip` es lo que querés. No cambies los períodos existentes en esta tarea (eso es scope de 1.4).

### Tarea 1.7 — Mejor selección de `local_ip` (privada, no-virtual, recalculable)
**Problema.** `compute_local_ip()` usa `local_ip_address::local_ip()`, que devuelve la IP de la ruta de menor métrica — a menudo una NIC virtual (WSL, Hyper-V, VPN). Eso hace que registremos/anunciemos por la interfaz equivocada y que el probe TCP y `derive_subnet_broadcast` partan de una IP inútil. Además `local_ip` se computa una sola vez por run y no se recalcula ante cambios de red.

**Archivo(s).** `src-tauri/src/identity.rs:109-113` (`compute_local_ip`), consumido en `identity.rs:45` y `:76`; usado luego en `discovery.rs:240` (`enable_interface`) y `udp_discovery.rs` (`local_ip`).

**Estado actual.**
```rust
fn compute_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}
```

**Cambio.**
1. Reemplazar `compute_local_ip` por una selección que enumere todas las interfaces y elija la mejor candidata privada no-virtual. Usar el crate `if-addrs` (`if_addrs::get_if_addrs() -> io::Result<Vec<Interface>>`; cada `Interface` tiene `.name: String`, `.addr: IfAddr`, y `.is_loopback()`). Algoritmo:
```rust
fn compute_local_ip() -> String {
    use std::net::Ipv4Addr;
    let is_private = |ip: &Ipv4Addr| ip.is_private(); // 10/8, 172.16/12, 192.168/16
    let is_virtual = |name: &str| {
        let n = name.to_lowercase();
        n.contains("vethernet") || n.contains("wsl") || n.contains("hyper-v")
            || n.contains("virtualbox") || n.contains("vmware") || n.contains("vpn")
            || n.contains("tailscale") || n.contains("zerotier") || n.contains("docker")
            || n.contains("loopback") || n.contains("tun") || n.contains("tap")
    };
    let ifaces = match if_addrs::get_if_addrs() { Ok(v) => v, Err(_) => Vec::new() };
    // 1ª pasada: IPv4 privada, no-loopback, no-virtual, interfaz up.
    let mut best: Option<Ipv4Addr> = None;
    for iface in &ifaces {
        if iface.is_loopback() || is_virtual(&iface.name) { continue; }
        if let if_addrs::IfAddr::V4(v4) = &iface.addr {
            if is_private(&v4.ip) { best = Some(v4.ip); break; }
        }
    }
    // Fallback al comportamiento viejo si no encontramos nada limpio.
    best.map(|ip| ip.to_string())
        .unwrap_or_else(|| local_ip_address::local_ip()
            .map(|ip| ip.to_string()).unwrap_or_default())
}
```
   Añadir `if-addrs` a `src-tauri/Cargo.toml` (`if-addrs = "0.13"`; confirmá la última versión estable disponible, ver `00-SHARED-CONTEXT.md` para la política de dependencias). `Ipv4Addr::is_private()` es stable desde hace tiempo, no requiere feature flags.
2. **Recalcular en cambio de red.** Agregar una función pública `pub fn compute_local_ip() -> String` (hacerla `pub` o exponer un helper) y llamarla ante un cambio de red. La forma mínima sin dependencias nuevas: un tick barato (p. ej. cada 30 s, con `MissedTickBehavior::Skip`) que recomputa la IP y, si cambió, re-registra el servicio mDNS (`register_self`) y reinicia el broadcaster UDP con el nuevo `local_ip`. Si querés un disparo por evento en vez de polling, el crate `if-watch` (`if_watch::tokio::IfWatcher`) emite `IfEvent::Up/Down` por interfaz; suscribirse ahí evita el poll. Para esta fase, el poll de 30 s es aceptable y suficiente; documentar el TODO de `if-watch` como mejora futura.

**Por qué.** Anunciar por la NIC física correcta es prerrequisito de todo lo demás: si `local_ip` apunta a WSL, el `enable_interface(IfKind::Addr(local_ip))` de `discovery.rs:241` habilita la interfaz equivocada y ningún peer nos ve. `is_private()` filtra APIPA/link-local y públicas; el filtro por nombre saca las virtuales que sí tienen IP privada (WSL usa `172.x`).

**Cuidado con.** **No cambies el wire format de `DiscoveryPacket`** — `local_ip` no viaja en el paquete UDP (la IP real la infiere el receptor del socket de origen, Tarea 1.1), así que un peer viejo sigue interoperando. La lista de nombres virtuales es heurística: en Windows los adaptadores de VPN/Hyper-V suelen contener "vEthernet", "Hyper-V", "VPN"; mantené la lista extensible pero conservadora (mejor caer al fallback viejo que elegir loopback). No rompas el caso Android: `get_if_addrs()` funciona en Android, pero si devuelve solo `wlan0` privado, el algoritmo lo elige bien. Mantené el fallback a `local_ip_address::local_ip()` para no regresar a IP vacía.

## Cómo verificar
Comandos de build/run/test están en `00-SHARED-CONTEXT.md`; acá va qué observar.

1. **Build y lint.** `cargo build` y `cargo clippy` en `src-tauri/` deben quedar limpios (sin warnings de bindings sin usar: `daemon_for_poll`, `my_ip_poll`, `subnet_prefix_24` deben haber desaparecido si aplicaste 1.2/1.3).
2. **CPU en reposo (root cause del ticket).** Con dos peers en la misma LAN y estables, abrir el Administrador de tareas (Task Manager) y observar el proceso de la app: el uso de CPU en reposo debe caer notablemente respecto de v0.15.0 (ya no hay barrido TCP de N peers cada 6 s). Confirmar en `runtime_log` la **ausencia** de líneas `[poll] probe ...` recurrentes cuando ambos peers se ven por UDP (solo deben aparecer probes on-demand o de backoff para peers ausentes).
3. **Fin del parpadeo.** Provocar el escenario clásico: un peer con WSL/Hyper-V activo. Antes, el log mostraba `[udp] IP DISAGREEMENT ...` seguido de `[poll] probe failed` / `DROPPED` en bucle. Después de 1.1, el log debe mostrar `[udp] correcting IP for <fp>: <wsl-ip> -> <real-ip> (datagram src wins)` **una vez**, y el peer debe permanecer online sin flapear. Verificar en la UI que el peer no parpadea (evento `peers-changed` deja de dispararse en loop).
4. **Reaper.** Matar bruscamente un peer (cerrar el proceso). El otro debe marcarlo offline en ~`peer_ttl` (≈15 s), no antes (no debe caer entre dos hellos UDP) y no "nunca". Buscar en el log la remoción emitida por el reaper.
5. **Sin gate /24.** Poner dos peers en máscara no-`/24` (p. ej. `/16` con octetos distintos en el tercer byte). Antes se descartaban con `different /24 (unreachable)`; después deben verse. Confirmar que ese string de log **ya no existe**.
6. **Wire compat.** Un peer con el binario nuevo y otro con v0.15.0 (wire UDP viejo) deben seguir descubriéndose: el `DiscoveryPacket` no cambió de campos. Verificar en el log `[udp] NEW peer ...` cruzado en ambos sentidos.
7. **Test unitario a agregar.** En `identity.rs`, un `#[test]` para el helper de selección: dado un set sintético de interfaces (mockear la lista o extraer la lógica de `is_virtual`/`is_private` a una fn pura testeable `fn pick_ip(ifaces: &[(String, Ipv4Addr, bool /*loopback*/)]) -> Option<Ipv4Addr>`), afirmar que ante `[("vEthernet (WSL)", 172.20.0.1, false), ("Wi-Fi", 192.168.1.42, false), ("lo", 127.0.0.1, true)]` devuelve `192.168.1.42`. Además un test de `PeerRecord`: tras aplicar una corrección UDP con `src_ip` distinto, `confirmed == true` y una posterior "actualización mDNS" con otra IP **no** cambia `ip` (extraé la lógica de reconciliación a una fn pura `fn reconcile_mdns(existing: &mut PeerRecord, ip, port, ...)` para poder testearla sin red).

## Riesgo y rollback
- **Qué puede romperse.** (a) Si el filtro de interfaces de la Tarea 1.7 es demasiado agresivo y descarta la única NIC válida, `local_ip` cae al fallback viejo — degradación, no rotura. (b) Si `peer_ttl` (Tarea 1.4) quedara **menor** que el intervalo UDP, los peers parpadearían al revés (offline entre hellos); por eso es 3×. (c) Quitar el `browse()` por tick (1.2) es seguro salvo que el `receiver` inicial se hubiera cerrado — se mantiene el browse de arranque, así que no hay regresión.
- **Cómo revertir.** Cada tarea es un cambio acotado a `discovery.rs`/`udp_discovery.rs`/`identity.rs`; revertir un `git` por archivo/tarea alcanza. La Tarea 1.6 (`MissedTickBehavior::Skip`) es la más segura y se puede shippear sola de inmediato (una línea por interval, sin efectos de comportamiento salvo menos ráfagas).
- **Orden seguro de ship independiente.** 1.6 (trivial) → 1.5 (defensivo) → 1.2/1.3 (borrados de código muerto/incorrecto) → 1.1 (política de reconciliación, el corazón) → 1.4 (rediseño del poller, el de mayor superficie) → 1.7 (selección de IP, depende de crate nuevo). 1.1 y 1.4 conviene shippearlas juntas: 1.4 asume el campo `confirmed`/`last_seen` que introduce 1.1. 1.7 es ortogonal y puede ir por separado.
