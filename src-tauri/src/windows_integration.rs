// Millennium Clipboard — Windows-specific integration helpers
//
// On Windows, portable apps (no MSIX/NSIS installer) don't show toast
// notifications by default because the OS routes them through an
// AppUserModelID (AUMID) registered with the system. Without a
// registered AUMID the toast is silently dropped.
//
// We work around that by writing the minimal AUMID record into
// HKCU\Software\Classes\AppUserModelId\<id> at startup. Per Microsoft
// docs that's enough for `IUserNotificationManager` (and by extension
// `tauri-plugin-notification`) to accept the toast.
//
// The whole file is `#[cfg(target_os = "windows")]` so nothing here
// gets compiled on Linux/macOS builds.

#![cfg(target_os = "windows")]

use std::path::Path;

const AUMID: &str = "com.guidocameraeq.millennium";
const DISPLAY_NAME: &str = "Millennium Clipboard";

/// Register the AUMID so Windows treats this portable .exe as a
/// "real" app for the purposes of toast notifications.
pub fn register_aumid_for_notifications(icon_path: Option<&Path>) {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!("Software\\Classes\\AppUserModelId\\{}", AUMID);
    let (key, _) = match hkcu.create_subkey(&key_path) {
        Ok(k) => k,
        Err(e) => {
            crate::runtime_log::warn(format!(
                "[win] could not create AUMID key '{}': {}",
                key_path, e
            ));
            return;
        }
    };

    if let Err(e) = key.set_value("DisplayName", &DISPLAY_NAME) {
        crate::runtime_log::warn(format!("[win] AUMID DisplayName write failed: {}", e));
    }
    if let Some(p) = icon_path {
        if p.exists() {
            let icon_str = p.to_string_lossy().to_string();
            if let Err(e) = key.set_value("IconUri", &icon_str) {
                crate::runtime_log::warn(format!("[win] AUMID IconUri write failed: {}", e));
            }
        }
    }
    crate::runtime_log::info(format!(
        "[win] AUMID registered: HKCU\\Software\\Classes\\AppUserModelId\\{}",
        AUMID
    ));

    // The notification API also wants the process's AUMID set so it
    // can correlate the toast with the registry entry. We call
    // SetCurrentProcessExplicitAppUserModelID via the windows crate.
    set_current_process_aumid();
}

/// Clean up the legacy "Send To" shortcut at
/// `%APPDATA%\Microsoft\Windows\SendTo\Millennium Clipboard.lnk` that
/// v0.10.0 used to install. Removed entirely in v0.10.1 — this runs
/// once on boot so any user that toggled it on previously gets it
/// cleared without having to do anything.
pub fn cleanup_legacy_send_to_shortcut() {
    let Some(roaming) = std::env::var_os("APPDATA") else { return };
    let p = std::path::PathBuf::from(roaming)
        .join("Microsoft")
        .join("Windows")
        .join("SendTo")
        .join("Millennium Clipboard.lnk");
    if p.exists() {
        match std::fs::remove_file(&p) {
            Ok(_) => crate::runtime_log::info(format!(
                "[win] removed legacy Send To shortcut: {}",
                p.display()
            )),
            Err(e) => crate::runtime_log::warn(format!(
                "[win] could not remove legacy Send To shortcut {}: {}",
                p.display(),
                e
            )),
        }
    }
}

/// Kill any other `millennium-clipboard.exe` process whose PID differs
/// from ours. Called once at startup to clear zombies left over from a
/// crashed previous launch (those zombies typically still hold the
/// HTTPS port but no longer respond, which used to surface as the
/// "another instance running" banner). Single-instance plugin handles
/// the normal case; this covers the crash-recovery case.
pub fn kill_other_millennium_processes() {
    // Dev double-launch coordinates ports via MILLENNIUM_INSTANCE and runs
    // two live instances on purpose — never let one kill its twin.
    if std::env::var("MILLENNIUM_INSTANCE")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some()
    {
        crate::runtime_log::info(
            "[win] MILLENNIUM_INSTANCE set — skipping zombie cleanup (dev double-launch)",
        );
        return;
    }

    let our_pid = std::process::id();
    let port = crate::discovery::DEFAULT_PORT;
    // Identify OUR processes by both exe names (deployed release is renamed
    // 'Millennium Clipboard.exe', a fresh build is 'millennium-clipboard.exe').
    // We only ever kill a PID that is one of OURS:
    //  - by name (covers a zombie that already released the port but lingers);
    //  - the owner of the app port 53319, but ONLY if that owner is one of our
    //    processes — so we never force-kill an unrelated app that happens to
    //    be listening on 53319. No wildcard match. Own PID always excluded.
    let ps_cmd = format!(
        r#"$ErrorActionPreference='SilentlyContinue';
$our={pid};
$ours=@(Get-Process -Name 'Millennium Clipboard','millennium-clipboard' | Select-Object -ExpandProperty Id);
$targets=@();
$targets += $ours;
Get-NetTCPConnection -LocalPort {port} -State Listen | ForEach-Object {{ if ($ours -contains $_.OwningProcess) {{ $targets += $_.OwningProcess }} }};
$targets | Sort-Object -Unique | Where-Object {{ $_ -and $_ -ne $our }} | ForEach-Object {{ Stop-Process -Id $_ -Force; Write-Output $_ }}"#,
        pid = our_pid,
        port = port
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_cmd])
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let killed_pids: Vec<String> = stdout
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if !killed_pids.is_empty() {
                crate::runtime_log::info(format!(
                    "[win] killed {} stale Millennium process(es): [{}] (our PID={}, port={})",
                    killed_pids.len(),
                    killed_pids.join(", "),
                    our_pid,
                    port
                ));
                // Give Windows a beat to release the TCP port held by
                // the dead processes.
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        }
        Err(e) => {
            crate::runtime_log::warn(format!("[win] zombie cleanup failed: {}", e));
        }
    }
}

/// Force-apply our `.ico` to the given HWND via Win32 `WM_SETICON`.
/// Tauri's `WebviewWindow::set_icon` on Windows reportedly updates the
/// taskbar/alt-tab "big icon" but not the title-bar "small icon", so
/// we set both explicitly here. `wry` doesn't expose this, so we go
/// direct.
pub fn apply_window_icon_win32(hwnd: isize) {
    use std::os::windows::ffi::OsStrExt;

    // Embedded copy of icon.ico — written to %TEMP% so LoadImageW
    // (which wants a file path with LR_LOADFROMFILE) can read it.
    const ICO_BYTES: &[u8] = include_bytes!("../icons/icon.ico");
    let temp_path = std::env::temp_dir().join("millennium_window_icon.ico");
    if !temp_path.exists()
        || std::fs::metadata(&temp_path)
            .map(|m| m.len() as usize != ICO_BYTES.len())
            .unwrap_or(true)
    {
        if let Err(e) = std::fs::write(&temp_path, ICO_BYTES) {
            crate::runtime_log::warn(format!(
                "[win] could not extract icon to {}: {}",
                temp_path.display(),
                e
            ));
            return;
        }
    }

    let path_wide: Vec<u16> = std::ffi::OsStr::new(&temp_path)
        .encode_wide()
        .chain(Some(0))
        .collect();

    const IMAGE_ICON: u32 = 1;
    const LR_LOADFROMFILE: u32 = 0x00000010;
    const LR_DEFAULTSIZE: u32 = 0x00000040;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn LoadImageW(
            hinst: isize,
            name: *const u16,
            type_: u32,
            cx: i32,
            cy: i32,
            fuload: u32,
        ) -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    let hicon_small = unsafe {
        LoadImageW(
            0,
            path_wide.as_ptr(),
            IMAGE_ICON,
            16,
            16,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        )
    };
    let hicon_big = unsafe {
        LoadImageW(
            0,
            path_wide.as_ptr(),
            IMAGE_ICON,
            32,
            32,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        )
    };

    if hicon_small == 0 && hicon_big == 0 {
        crate::runtime_log::warn(
            "[win] LoadImageW returned NULL for both icon sizes — header will fallback",
        );
        return;
    }

    if hicon_small != 0 {
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon_small);
        }
    }
    if hicon_big != 0 {
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, hicon_big);
        }
    }
    crate::runtime_log::info(format!(
        "[win] WM_SETICON applied (small=0x{:x} big=0x{:x})",
        hicon_small as usize, hicon_big as usize
    ));
}

fn set_current_process_aumid() {
    // Minimal SetCurrentProcessExplicitAppUserModelID call without
    // pulling the full `windows` crate. We use raw extern "system".
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(AUMID).encode_wide().chain(Some(0)).collect();
    let hr = unsafe { SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
    if hr != 0 {
        crate::runtime_log::warn(format!(
            "[win] SetCurrentProcessExplicitAppUserModelID returned 0x{:08X}",
            hr
        ));
    }
}
