// Millennium Clipboard — HTTPS client (Fase 5–7)
//
// Cert chain validation is intentionally skipped — we cross-check the
// peer fingerprint via /info before sending, and ignore CN/SAN since
// peers identify themselves by fingerprint, not hostname.
//
// IMPORTANT: a single Client is shared across all sends so the
// connection is pooled (avoids LocalSend bug #1657: 7000 small files
// collapse to 80 KB/s when each upload pays the TLS handshake).

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::io::ReaderStream;

const CHUNK: usize = 64 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

// ---------------------------------------------------------------------------
// Shared HTTP client (pooled, kept warm)
// ---------------------------------------------------------------------------

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .pool_max_idle_per_host(8)
            .timeout(Duration::from_secs(300))
            .build()
            .expect("build reqwest client")
    })
}

// ---------------------------------------------------------------------------
// /info probe (Fase 4)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub version: String,
    pub protocol: String,
}

pub async fn fetch_info(ip: &str, port: u16) -> Result<RemoteInfo> {
    let url = format!("https://{}:{}/info", ip, port);
    let resp = client()
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        bail!("info endpoint returned {}", resp.status());
    }
    resp.json::<RemoteInfo>().await.context("decode /info JSON")
}

// ---------------------------------------------------------------------------
// /text (Fase 5)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TextPayload {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    sender_port: u16,
}

pub async fn post_text(
    ip: &str,
    port: u16,
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    sender_port: u16,
) -> Result<()> {
    let url = format!("https://{}:{}/text", ip, port);
    let resp = client()
        .post(&url)
        .json(&TextPayload { text, sender_alias, sender_fingerprint, sender_port })
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;
    if !resp.status().is_success() {
        bail!("text endpoint returned {}", resp.status());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// /prepare-upload + /upload (Fase 7)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrepareFile {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub mime: Option<String>,
    pub sha256: Option<String>,
    pub rel_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareUploadRequest<'a> {
    session_id: &'a str,
    sender_alias: &'a str,
    sender_fingerprint: &'a str,
    sender_port: u16,
    files: &'a [PrepareFile],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    pub session_id: String,
    pub files: HashMap<String, String>,
}

pub async fn prepare_upload(
    ip: &str,
    port: u16,
    session_id: &str,
    sender_alias: &str,
    sender_fingerprint: &str,
    sender_port: u16,
    files: &[PrepareFile],
) -> Result<PrepareUploadResponse> {
    let url = format!("https://{}:{}/prepare-upload", ip, port);
    let resp = client()
        .post(&url)
        .json(&PrepareUploadRequest {
            session_id,
            sender_alias,
            sender_fingerprint,
            sender_port,
            files,
        })
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        bail!("recipient rejected the transfer");
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("prepare-upload returned {} {}", status, body);
    }
    resp.json::<PrepareUploadResponse>()
        .await
        .context("decode prepare-upload JSON")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SenderProgressEvent {
    session_id: String,
    file_id: String,
    bytes_sent: u64,
    total: u64,
}

/// Stream the given file's body to the peer. Emits
/// `transfer-progress-sender` events with bytes uploaded.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadProgress {
    bytes_received: u64,
    #[allow(dead_code)]
    total: u64,
    #[allow(dead_code)]
    completed: bool,
}

/// Best-effort: how many bytes does the receiver already have for this
/// file? Used to compute a Range start so we can resume an interrupted
/// upload instead of restarting from byte 0.
async fn fetch_upload_progress(
    ip: &str,
    port: u16,
    session_id: &str,
    file_id: &str,
    token: &str,
) -> u64 {
    let url = format!(
        "https://{}:{}/upload/{}/{}/progress?token={}",
        ip, port, session_id, file_id, token
    );
    match client().get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<UploadProgress>().await {
            Ok(p) => p.bytes_received,
            Err(_) => 0,
        },
        _ => 0,
    }
}

pub async fn upload_file(
    app: AppHandle,
    ip: &str,
    port: u16,
    session_id: &str,
    file_id: &str,
    token: &str,
    path: &Path,
    total: u64,
) -> Result<()> {
    // Up to 3 retries (~initial + 2 resume attempts). Failures within
    // a retry call `fetch_upload_progress` so the next attempt only
    // re-sends what the receiver doesn't have. Backoff is short — for
    // long Wi-Fi blackouts the user retries by hand.
    const MAX_ATTEMPTS: usize = 3;
    let mut attempt = 0usize;
    loop {
        attempt += 1;
        let resume_from = if attempt == 1 {
            0
        } else {
            fetch_upload_progress(ip, port, session_id, file_id, token).await
        };
        match upload_file_once(
            app.clone(),
            ip,
            port,
            session_id,
            file_id,
            token,
            path,
            total,
            resume_from,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MAX_ATTEMPTS => {
                eprintln!(
                    "[upload] attempt {}/{} for {} failed: {} — retrying",
                    attempt, MAX_ATTEMPTS, file_id, e
                );
                tokio::time::sleep(std::time::Duration::from_millis(800 * attempt as u64)).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn upload_file_once(
    app: AppHandle,
    ip: &str,
    port: u16,
    session_id: &str,
    file_id: &str,
    token: &str,
    path: &Path,
    total: u64,
    resume_from: u64,
) -> Result<()> {
    let url = format!(
        "https://{}:{}/upload/{}/{}?token={}",
        ip, port, session_id, file_id, token
    );

    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("open {}", path.display()))?;

    if resume_from > 0 && resume_from < total {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(resume_from))
            .await
            .with_context(|| format!("seek to {} in {}", resume_from, path.display()))?;
        eprintln!(
            "[upload] resuming {} from byte {} of {}",
            file_id, resume_from, total
        );
    } else if resume_from >= total && total > 0 {
        // The receiver already has the whole thing. Just confirm.
        let _ = app.emit(
            "transfer-progress-sender",
            &SenderProgressEvent {
                session_id: session_id.into(),
                file_id: file_id.into(),
                bytes_sent: total,
                total,
            },
        );
        return Ok(());
    }

    let progress = Arc::new(AtomicU64::new(resume_from));
    let progress_clone = progress.clone();
    let app_clone = app.clone();
    let sid = session_id.to_string();
    let fid = file_id.to_string();

    let last_emit = Arc::new(std::sync::Mutex::new(Instant::now()));
    let last_emit_clone = last_emit.clone();

    let reader = ReaderStream::with_capacity(file, CHUNK).map(move |chunk| {
        if let Ok(ref bytes) = chunk {
            let new_total = progress_clone.fetch_add(bytes.len() as u64, Ordering::Relaxed)
                + bytes.len() as u64;
            let mut last = last_emit_clone.lock().unwrap();
            if last.elapsed() > PROGRESS_EVERY {
                *last = Instant::now();
                drop(last);
                let _ = app_clone.emit(
                    "transfer-progress-sender",
                    &SenderProgressEvent {
                        session_id: sid.clone(),
                        file_id: fid.clone(),
                        bytes_sent: new_total,
                        total,
                    },
                );
            }
        }
        chunk
    });

    let remaining = total.saturating_sub(resume_from);
    let mut req = client()
        .post(&url)
        .header("content-length", remaining.to_string());
    if resume_from > 0 {
        req = req.header("range", format!("bytes={}-", resume_from));
    }
    let resp = req
        .body(reqwest::Body::wrap_stream(reader))
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;

    let _ = app.emit(
        "transfer-progress-sender",
        &SenderProgressEvent {
            session_id: session_id.into(),
            file_id: file_id.into(),
            bytes_sent: progress.load(Ordering::Relaxed),
            total,
        },
    );

    if !resp.status().is_success() {
        bail!("upload returned {}", resp.status());
    }
    Ok(())
}

pub async fn cancel_upload(ip: &str, port: u16, session_id: &str) -> Result<()> {
    let url = format!("https://{}:{}/cancel/{}", ip, port, session_id);
    let _ = client().post(&url).send().await; // best-effort
    Ok(())
}

// ---------------------------------------------------------------------------
// /clipboard (v0.6.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardPayload<'a> {
    text: &'a str,
    sender_alias: &'a str,
    sender_fingerprint: &'a str,
}

pub async fn post_clipboard(
    ip: &str,
    port: u16,
    text: &str,
    sender_alias: &str,
    sender_fingerprint: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/clipboard", ip, port);
    let resp = client()
        .post(&url)
        .json(&ClipboardPayload { text, sender_alias, sender_fingerprint })
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        // Peer hasn't opted into sync with us — silent skip.
        return Ok(());
    }
    if !resp.status().is_success() {
        bail!("clipboard endpoint returned {}", resp.status());
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardImagePayload<'a> {
    png_base64: &'a str,
    sender_alias: &'a str,
    sender_fingerprint: &'a str,
}

pub async fn post_clipboard_image(
    ip: &str,
    port: u16,
    png_base64: &str,
    sender_alias: &str,
    sender_fingerprint: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/clipboard/image", ip, port);
    let resp = client()
        .post(&url)
        .json(&ClipboardImagePayload {
            png_base64,
            sender_alias,
            sender_fingerprint,
        })
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::NOT_FOUND
    {
        // Either the peer didn't enable sync, or it's an older client
        // without the image endpoint — silent skip in both cases.
        return Ok(());
    }
    if !resp.status().is_success() {
        bail!("clipboard/image endpoint returned {}", resp.status());
    }
    Ok(())
}
