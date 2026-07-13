// Millennium Clipboard — auto-updater (v0.5.0 F5)
//
// Polls the GitHub Releases API for a newer version, downloads the new
// portable .exe to a temp dir, and hands off to a tiny .bat script that
// swaps the binary and relaunches once the current process exits.
//
// Code signing / signature verification is intentionally NOT implemented
// for this alpha — adding it later requires generating an offline key
// pair and signing each release. See the project roadmap.

use anyhow::{bail, Context, Result};
use serde::Serialize;

const REPO: &str = "guidocameraeq/Millennium-Clipboard";

/// Pick which asset of a GitHub release is the portable Windows
/// executable. Historically (v0.8.x – v0.10.0) we named the asset
/// `Millennium-Clipboard_<ver>_portable.exe`, so we prefer that suffix
/// when present (older releases the user might have installed). From
/// v0.11.0 forward the asset is simply `Millennium Clipboard.exe`
/// (the version lives in metadata + the release tag), so as a
/// fallback we accept any `.exe` asset.
fn pick_release_asset(assets: &[serde_json::Value]) -> Option<&serde_json::Value> {
    let by_suffix = |suffix: &str| {
        assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.ends_with(suffix))
                .unwrap_or(false)
        })
    };
    #[cfg(target_os = "android")]
    {
        by_suffix(".apk")
    }
    #[cfg(not(target_os = "android"))]
    {
        by_suffix("portable.exe").or_else(|| by_suffix(".exe"))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub download_url: Option<String>,
    pub release_url: String,
    pub release_notes: String,
}

pub async fn check_for_update() -> Result<UpdateInfo> {
    // Use the full releases list (not /releases/latest) because GitHub
    // returns 404 from /latest when every release is marked as
    // prerelease — which is our case until v1.0.0.
    let url = format!("https://api.github.com/repos/{}/releases?per_page=30", REPO);
    let client = reqwest::Client::builder()
        .user_agent(concat!("Millennium-Clipboard/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("build http client")?;

    let releases: Vec<serde_json::Value> = client
        .get(&url)
        .send()
        .await
        .context("query GitHub releases")?
        .error_for_status()
        .context("GitHub returned non-2xx")?
        .json()
        .await
        .context("decode GitHub response")?;

    // Filter out drafts; keep prereleases (we always publish those).
    let release = releases
        .into_iter()
        .find(|r| !r["draft"].as_bool().unwrap_or(false))
        .ok_or_else(|| anyhow::anyhow!("no releases published yet"))?;

    let latest_tag = release["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let release_url = release["html_url"].as_str().unwrap_or("").to_string();
    let release_notes = release["body"].as_str().unwrap_or("").to_string();

    let download_url = release["assets"]
        .as_array()
        .and_then(|arr| pick_release_asset(arr.as_slice()))
        .and_then(|a| a["browser_download_url"].as_str().map(String::from));

    let current = env!("CARGO_PKG_VERSION").to_string();
    let has_update = version_gt(&latest_tag, &current);

    Ok(UpdateInfo {
        current_version: current,
        latest_version: latest_tag,
        has_update,
        download_url,
        release_url,
        release_notes,
    })
}

fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split(['.', '-'])
            .take(3)
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    for i in 0..av.len().max(bv.len()) {
        let ai = av.get(i).copied().unwrap_or(0);
        let bi = bv.get(i).copied().unwrap_or(0);
        if ai > bi {
            return true;
        }
        if ai < bi {
            return false;
        }
    }
    false
}

/// Download the new .exe to a temp file, write a swap-and-restart batch
/// script next to it, spawn the script detached. The caller should then
/// exit the app so the script can move the file in place.
#[cfg(target_os = "windows")]
pub async fn download_and_stage(download_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Millennium-Clipboard/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let bytes = client
        .get(download_url)
        .send()
        .await
        .context("download new exe")?
        .error_for_status()?
        .bytes()
        .await
        .context("read new exe body")?;

    let current_exe = std::env::current_exe().context("locate current exe")?;
    let temp_dir = std::env::temp_dir();
    let staged = temp_dir.join("millennium-clipboard-update.exe");
    let script = temp_dir.join("millennium-clipboard-update.bat");

    tokio::fs::write(&staged, &bytes)
        .await
        .with_context(|| format!("write {}", staged.display()))?;

    // Self-deleting batch: wait → RETRY the swap in a loop (the old .exe can
    // stay locked for a beat by AV/handle release) → on success launch the new
    // exe and clear any stale failure marker; on persistent failure write a
    // marker the app surfaces at next boot so the update never fails silently.
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
    tokio::fs::write(&script, bat)
        .await
        .with_context(|| format!("write {}", script.display()))?;

    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("cmd")
        .arg("/C")
        .arg(&script)
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .context("spawn updater batch")?;
    Ok(())
}

/// Android: download the new APK to a path the OS package installer can
/// read, and return that path so the frontend can hand it off to
/// `tauri-plugin-opener` (which triggers an ACTION_VIEW intent that
/// brings up Android's "Install app?" dialog). Sideload-style — the
/// user must have already enabled "Install unknown apps" for our app.
/// Best-effort version extraction from a GitHub release asset URL.
/// Falls back to "update" so we always have a filename suffix to use.
#[cfg(target_os = "android")]
pub fn version_for_filename(download_url: &str) -> String {
    // GitHub asset URLs look like:
    //   https://github.com/.../releases/download/v0.15.0/Millennium%20Clipboard.apk
    download_url
        .split("/releases/download/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|tag| tag.trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "update".to_string())
}

#[cfg(target_os = "android")]
pub async fn download_and_stage_apk(download_url: &str, cache_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Millennium-Clipboard/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let bytes = client
        .get(download_url)
        .send()
        .await
        .context("download new apk")?
        .error_for_status()?
        .bytes()
        .await
        .context("read new apk body")?;

    tokio::fs::create_dir_all(cache_dir)
        .await
        .with_context(|| format!("mkdir {}", cache_dir.display()))?;
    let apk_path = cache_dir.join("millennium-update.apk");
    tokio::fs::write(&apk_path, &bytes)
        .await
        .with_context(|| format!("write {}", apk_path.display()))?;
    Ok(apk_path)
}

#[cfg(not(any(target_os = "windows", target_os = "android")))]
pub async fn download_and_stage(_download_url: &str) -> Result<()> {
    bail!("auto-update is only supported on Windows and Android in this build");
}
