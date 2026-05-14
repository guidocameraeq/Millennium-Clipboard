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

/// Path to the per-user "Send To" folder
/// (`%APPDATA%\Microsoft\Windows\SendTo\`).
fn send_to_dir() -> Option<std::path::PathBuf> {
    let roaming = std::env::var_os("APPDATA")?;
    Some(
        std::path::PathBuf::from(roaming)
            .join("Microsoft")
            .join("Windows")
            .join("SendTo"),
    )
}

fn send_to_shortcut_path() -> Option<std::path::PathBuf> {
    Some(send_to_dir()?.join("Millennium Clipboard.lnk"))
}

/// Create a shortcut in the user's Send To folder so right-clicking
/// files in Explorer → Send to → Millennium Clipboard launches our exe
/// with those paths as args. Uses PowerShell + WScript.Shell so we
/// don't have to pull in a COM crate.
pub fn install_send_to_shortcut() {
    let Some(target) = std::env::current_exe().ok() else {
        crate::runtime_log::warn("[win] could not resolve current_exe for Send To shortcut");
        return;
    };
    let Some(shortcut) = send_to_shortcut_path() else { return };

    let ps = format!(
        r#"$w = New-Object -ComObject WScript.Shell;
$s = $w.CreateShortcut('{lnk}');
$s.TargetPath = '{exe}';
$s.Arguments = '';
$s.WorkingDirectory = '{cwd}';
$s.IconLocation = '{exe},0';
$s.Description = 'Millennium Clipboard';
$s.Save()"#,
        lnk = shortcut.to_string_lossy().replace('\'', "''"),
        exe = target.to_string_lossy().replace('\'', "''"),
        cwd = target
            .parent()
            .map(|p| p.to_string_lossy().replace('\'', "''"))
            .unwrap_or_default(),
    );
    let res = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output();
    match res {
        Ok(o) if o.status.success() => {
            crate::runtime_log::info(format!(
                "[win] Send To shortcut installed at {}",
                shortcut.display()
            ));
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            crate::runtime_log::warn(format!(
                "[win] Send To shortcut create failed: {}",
                stderr.trim()
            ));
        }
        Err(e) => {
            crate::runtime_log::warn(format!("[win] powershell exec failed: {}", e));
        }
    }
}

pub fn remove_send_to_shortcut() {
    if let Some(p) = send_to_shortcut_path() {
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                crate::runtime_log::warn(format!(
                    "[win] could not remove Send To shortcut {}: {}",
                    p.display(),
                    e
                ));
            } else {
                crate::runtime_log::info(format!(
                    "[win] Send To shortcut removed: {}",
                    p.display()
                ));
            }
        }
    }
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
