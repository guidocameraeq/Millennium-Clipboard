// Millennium Clipboard — in-memory runtime log
//
// A circular buffer of recent log lines that can be (a) emitted live to
// the frontend as `log-line` events, and (b) dumped wholesale via the
// `get_runtime_log` Tauri command. This is the diagnostic surface the
// user pastes back when something misbehaves in the wild.
//
// All log entries also go to stderr — useful when running from a
// terminal — but on a release Windows build with `windows_subsystem =
// "windows"` stderr is effectively a black hole, so the buffer is the
// real channel.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

const CAPACITY: usize = 5000;

struct RuntimeLog {
    lines: Mutex<VecDeque<String>>,
    app: Mutex<Option<AppHandle>>,
}

static LOG: OnceLock<RuntimeLog> = OnceLock::new();

fn store() -> &'static RuntimeLog {
    LOG.get_or_init(|| RuntimeLog {
        lines: Mutex::new(VecDeque::with_capacity(CAPACITY)),
        app: Mutex::new(None),
    })
}

pub fn bind_app(app: AppHandle) {
    *store().app.lock().unwrap() = Some(app);
}

fn iso_now() -> String {
    // Lightweight HH:MM:SS.mmm timestamp — UTC offset doesn't help here,
    // the user just wants a relative timeline. UNIX seconds % day gives a
    // wall-clock-ish reading with no chrono dep.
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
}

pub fn push(level: &str, msg: String) {
    let line = format!("{} [{}] {}", iso_now(), level, msg);
    eprintln!("{}", line);
    let s = store();
    {
        let mut lines = s.lines.lock().unwrap();
        if lines.len() >= CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line.clone());
    }
    if let Some(app) = s.app.lock().unwrap().as_ref() {
        let _ = app.emit("log-line", &line);
    }
}

pub fn info(msg: impl Into<String>) {
    push("INFO", msg.into());
}

pub fn warn(msg: impl Into<String>) {
    push("WARN", msg.into());
}

pub fn err(msg: impl Into<String>) {
    push("ERR ", msg.into());
}

pub fn dump_all() -> String {
    let s = store();
    let lines = s.lines.lock().unwrap();
    lines.iter().cloned().collect::<Vec<_>>().join("\n")
}

pub fn clear() {
    let s = store();
    s.lines.lock().unwrap().clear();
}

/// Snapshot every routable IPv4 interface the OS exposes. We need this
/// to diagnose the "local_ip pointed at WSL/Hyper-V instead of WiFi"
/// class of bug — without it, we can't tell whether the wrong NIC was
/// picked.
pub fn log_network_interfaces() {
    match local_ip_address::list_afinet_netifas() {
        Ok(list) => {
            info(format!("[net] {} IPv4 interface(s) detected:", list.len()));
            for (name, ip) in list {
                info(format!("[net]   {} -> {}", name, ip));
            }
        }
        Err(e) => {
            err(format!("[net] list_afinet_netifas failed: {}", e));
        }
    }
}
