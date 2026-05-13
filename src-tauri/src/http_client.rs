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
    let url = format!(
        "https://{}:{}/upload/{}/{}?token={}",
        ip, port, session_id, file_id, token
    );

    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("open {}", path.display()))?;

    let progress = Arc::new(AtomicU64::new(0));
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

    let resp = client()
        .post(&url)
        .header("content-length", total.to_string())
        .body(reqwest::Body::wrap_stream(reader))
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;

    // Final progress tick (in case the throttle skipped the last chunk)
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
