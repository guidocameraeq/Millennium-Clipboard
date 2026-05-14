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
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

const CAPACITY: usize = 5000;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

struct RuntimeLog {
    lines: Mutex<VecDeque<String>>,
    app: Mutex<Option<AppHandle>>,
    file: Mutex<Option<File>>,
}

static LOG: OnceLock<RuntimeLog> = OnceLock::new();

fn store() -> &'static RuntimeLog {
    LOG.get_or_init(|| RuntimeLog {
        lines: Mutex::new(VecDeque::with_capacity(CAPACITY)),
        app: Mutex::new(None),
        file: Mutex::new(None),
    })
}

pub fn bind_app(app: AppHandle) {
    *store().app.lock().unwrap() = Some(app);
}

/// Open `<data_dir>/runtime.log` for appending. If the existing file
/// is over MAX_FILE_BYTES, rotate it to `runtime.log.1` first (single
/// backup, no chained rotation — keeps disk usage bounded to ~10MB).
pub fn bind_file(data_dir: &Path) {
    let log_path: PathBuf = data_dir.join("runtime.log");
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > MAX_FILE_BYTES {
            let backup = data_dir.join("runtime.log.1");
            let _ = std::fs::remove_file(&backup);
            let _ = std::fs::rename(&log_path, &backup);
        }
    }
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(mut f) => {
            let _ = writeln!(
                f,
                "\n========== new run @ {} ==========",
                iso_now()
            );
            *store().file.lock().unwrap() = Some(f);
        }
        Err(e) => {
            eprintln!(
                "[runtime_log] could not open {}: {}",
                log_path.display(),
                e
            );
        }
    }
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
    if let Some(f) = s.file.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{}", line);
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
