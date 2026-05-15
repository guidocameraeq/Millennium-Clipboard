// Millennium Clipboard — Android FS bridge (v0.13.0)
//
// Wraps tauri-plugin-android-fs so the rest of our codebase doesn't
// have to know about content:// URIs or MediaStore. Two helpers:
//
//   * resolve_content_uri: takes the `content://...` URI that
//     tauri-plugin-dialog returns from the SAF picker, streams the
//     bytes into the app's private cache, and returns a real
//     filesystem path that tokio::fs can open. Lets us reuse the
//     existing send_files plumbing unchanged.
//
//   * save_to_gallery: writes PNG bytes to the public `Pictures/
//     Millennium/` folder and triggers a MediaStore scan so Gallery
//     / Photos pick up the file immediately. Used by the
//     `/clipboard/image` endpoint to give incoming images a visible
//     home (writing them straight into the OS clipboard is a v0.14
//     job that still needs FileProvider + JNI).

#![cfg(target_os = "android")]

use std::path::PathBuf;

use tauri::{AppHandle, Runtime};
use tauri_plugin_android_fs::{
    AndroidFsExt, FileUri, PublicImageDir, PrivateDir,
};

/// Copy a `content://` URI into <app_cache>/uploads/<displayName>
/// and return the resulting filesystem path. Falls back gracefully
/// if any step fails; the error string surfaces to the JS side.
pub async fn resolve_content_uri<R: Runtime>(
    app: &AppHandle<R>,
    uri_str: String,
) -> Result<PathBuf, String> {
    let api = app.android_fs_async();

    let uri = FileUri::from_uri(uri_str);
    let name = api
        .get_name(&uri)
        .await
        .map_err(|e| format!("get_name: {e}"))?;

    let cache_dir = api
        .private_storage()
        .resolve_path(PrivateDir::Cache)
        .await
        .map_err(|e| format!("resolve_path cache: {e}"))?
        .join("uploads");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir cache: {e}"))?;
    let dest = cache_dir.join(&name);

    let mut src = api
        .open_file_readable(&uri)
        .await
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

/// Write PNG bytes to the public `Pictures/Millennium/<filename>`
/// MediaStore entry and force-scan so Gallery / Photos see it
/// instantly. Returns the public URI string.
pub async fn save_image_to_gallery<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let api = app.android_fs_async();

    // No-op on Android 10+ if we only touch our own MediaStore entries,
    // but request to be polite on legacy devices.
    let _ = api.public_storage().request_permission().await;

    let uri = api
        .public_storage()
        .write_new(
            None,
            PublicImageDir::Pictures,
            &format!("Millennium/{filename}"),
            Some("image/png"),
            bytes,
        )
        .await
        .map_err(|e| format!("write_new: {e}"))?;

    let _ = api.public_storage().scan(&uri).await;

    Ok(uri.uri)
}
