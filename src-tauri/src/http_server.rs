// Millennium Clipboard — HTTPS server (Fase 4–7)
//
// Endpoints:
//   GET  /info                              identity probe
//   POST /text                              receive text
//   POST /prepare-upload                    file transfer kickoff (Fase 7)
//   POST /upload/:sessionId/:fileId         file body, streamed (Fase 7)
//   POST /cancel/:sessionId                 cancel from either side (Fase 7)

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::clipboard_sync::{hash_text, ClipboardSyncStore};
use crate::preferences::PreferencesStore;
use crate::settings::SettingsStore;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Public wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub version: String,
    pub protocol: String,
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ServerState {
    info: Arc<InfoResponse>,
    app: AppHandle,
    prefs: Arc<PreferencesStore>,
    settings: Arc<SettingsStore>,
    clipboard: Arc<ClipboardSyncStore>,
    sessions: Arc<Mutex<HashMap<String, IncomingSession>>>,
}

struct IncomingSession {
    sender_alias: String,
    sender_fingerprint: String,
    destination_dir: PathBuf,
    files: HashMap<String, Arc<IncomingFile>>,
    started_at: Instant,
    total_size: u64,
}

struct IncomingFile {
    name: String,
    rel_path: Option<String>,
    size: u64,
    sha256: Option<String>,
    token: String,
    bytes_received: AtomicU64,
    completed: AtomicBool,
}

// ---------------------------------------------------------------------------
// Server entry
// ---------------------------------------------------------------------------

/// Find the first free TCP port in `[start, start+max_tries)` by trying to
/// bind a temporary listener. The listener is dropped immediately so the
/// caller can rebind through axum-server. There is a tiny race window
/// where another process could steal the port between drop and rebind,
/// but in practice this is fine — and far better than failing silently.
pub fn find_free_tcp_port(start: u16, max_tries: u16) -> Option<u16> {
    for offset in 0..max_tries {
        let port = start + offset;
        match std::net::TcpListener::bind(("0.0.0.0", port)) {
            Ok(listener) => {
                drop(listener);
                return Some(port);
            }
            Err(_) => continue,
        }
    }
    None
}

pub async fn run(
    app: AppHandle,
    port: u16,
    info: InfoResponse,
    cert_pem: String,
    key_pem: String,
    prefs: Arc<PreferencesStore>,
    settings: Arc<SettingsStore>,
    clipboard: Arc<ClipboardSyncStore>,
) -> Result<()> {
    let state = ServerState {
        info: Arc::new(info),
        app,
        prefs,
        settings,
        clipboard,
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    let router = Router::new()
        .route("/info", get(handle_info))
        .route("/text", post(handle_text))
        .route("/prepare-upload", post(handle_prepare_upload))
        .route("/upload/{session_id}/{file_id}", post(handle_upload))
        .route(
            "/upload/{session_id}/{file_id}/progress",
            get(handle_upload_progress),
        )
        .route("/cancel/{session_id}", post(handle_cancel))
        .route("/clipboard", post(handle_clipboard))
        .route("/clipboard/image", post(handle_clipboard_image))
        .with_state(state);

    let tls = RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await
        .context("load TLS config")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let server = axum_server::bind_rustls(addr, tls);
    crate::runtime_log::info(format!("[http] HTTPS server now listening on {}", addr));
    server
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("axum server")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// /info
// ---------------------------------------------------------------------------

async fn handle_info(State(state): State<ServerState>) -> Json<InfoResponse> {
    Json((*state.info).clone())
}

// ---------------------------------------------------------------------------
// /text
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextPayload {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    #[serde(default)]
    sender_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingTextEvent {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    sender_ip: String,
    sender_port: u16,
    received_at: i64,
}

async fn handle_text(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<TextPayload>,
) -> StatusCode {
    let sender_port = payload.sender_port.unwrap_or(crate::discovery::DEFAULT_PORT);
    let evt = IncomingTextEvent {
        text: payload.text,
        sender_alias: payload.sender_alias,
        sender_fingerprint: payload.sender_fingerprint,
        sender_ip: addr.ip().to_string(),
        sender_port,
        received_at: unix_now(),
    };
    println!(
        "[http] /text received {} chars from {} ({})",
        evt.text.chars().count(),
        evt.sender_alias,
        evt.sender_ip
    );
    let _ = state.app.emit("incoming-text", &evt);
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// /prepare-upload
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PrepareFile {
    file_id: String,
    name: String,
    size: u64,
    #[serde(default)]
    mime: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    rel_path: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareUploadRequest {
    session_id: String,
    sender_alias: String,
    sender_fingerprint: String,
    #[serde(default)]
    sender_port: Option<u16>,
    files: Vec<PrepareFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareUploadResponse {
    session_id: String,
    files: HashMap<String, String>, // file_id -> token
}

/// Approval channel registry — shared between handlers (sender awaits;
/// approve/reject commands fire the result through this channel).
type ApprovalRegistry = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

// Lazily-initialized global. Simpler than threading through state — only
// touched by approval flow.
fn approval_registry() -> &'static ApprovalRegistry {
    use std::sync::OnceLock;
    static REG: OnceLock<ApprovalRegistry> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

pub fn resolve_approval(session_id: &str, approved: bool) -> bool {
    let reg = approval_registry();
    let tx = { reg.lock().unwrap().remove(session_id) };
    if let Some(tx) = tx {
        let _ = tx.send(approved);
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingFilesRequestEvent {
    session_id: String,
    sender_alias: String,
    sender_fingerprint: String,
    sender_ip: String,
    sender_port: u16,
    file_count: usize,
    total_size: u64,
    files: Vec<IncomingFilePreview>,
    auto_accepted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingFilePreview {
    file_id: String,
    name: String,
    size: u64,
    mime: Option<String>,
    rel_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<String>,
}

async fn handle_prepare_upload(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<PrepareUploadRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let total: u64 = req.files.iter().map(|f| f.size).sum();
    let sender_port = req.sender_port.unwrap_or(crate::discovery::DEFAULT_PORT);
    let sender_ip = addr.ip().to_string();
    println!(
        "[http] /prepare-upload from {} at {} ({} files, {} bytes)",
        req.sender_alias,
        sender_ip,
        req.files.len(),
        total
    );

    let settings = state.settings.snapshot();
    let is_fav = state.prefs.is_favorite(&req.sender_fingerprint);
    let auto_accept = is_fav && settings.auto_accept_favorites;

    // Build preview event regardless — we either auto-approve and notify,
    // or wait for the user to decide.
    let preview = IncomingFilesRequestEvent {
        session_id: req.session_id.clone(),
        sender_alias: req.sender_alias.clone(),
        sender_fingerprint: req.sender_fingerprint.clone(),
        sender_ip: sender_ip.clone(),
        sender_port,
        file_count: req.files.len(),
        total_size: total,
        files: req
            .files
            .iter()
            .map(|f| IncomingFilePreview {
                file_id: f.file_id.clone(),
                name: f.name.clone(),
                size: f.size,
                mime: f.mime.clone(),
                rel_path: f.rel_path.clone(),
                thumbnail: f.thumbnail.clone(),
            })
            .collect(),
        auto_accepted: auto_accept,
    };
    let _ = state.app.emit("incoming-files-request", &preview);

    let approved = if auto_accept {
        true
    } else {
        // Wait for user decision via approve_session / reject_session command.
        let (tx, rx) = oneshot::channel::<bool>();
        approval_registry()
            .lock()
            .unwrap()
            .insert(req.session_id.clone(), tx);

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                approval_registry().lock().unwrap().remove(&req.session_id);
                let _ = state.app.emit(
                    "incoming-files-timeout",
                    serde_json::json!({ "sessionId": req.session_id }),
                );
                false
            }
        }
    };

    if !approved {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "rejected" })),
        );
    }

    // Build session + generate per-file tokens.
    let mut files_map: HashMap<String, Arc<IncomingFile>> = HashMap::new();
    let mut tokens_response: HashMap<String, String> = HashMap::new();
    for f in &req.files {
        let token = Uuid::new_v4().simple().to_string();
        tokens_response.insert(f.file_id.clone(), token.clone());
        files_map.insert(
            f.file_id.clone(),
            Arc::new(IncomingFile {
                name: f.name.clone(),
                rel_path: f.rel_path.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                token,
                bytes_received: AtomicU64::new(0),
                completed: AtomicBool::new(false),
            }),
        );
    }

    let session = IncomingSession {
        sender_alias: req.sender_alias.clone(),
        sender_fingerprint: req.sender_fingerprint.clone(),
        destination_dir: settings.download_dir.clone(),
        files: files_map,
        started_at: Instant::now(),
        total_size: total,
    };

    state
        .sessions
        .lock()
        .unwrap()
        .insert(req.session_id.clone(), session);

    let _ = state.app.emit(
        "incoming-files-approved",
        serde_json::json!({ "sessionId": req.session_id }),
    );

    (
        StatusCode::OK,
        Json(serde_json::json!(PrepareUploadResponse {
            session_id: req.session_id.clone(),
            files: tokens_response,
        })),
    )
}

// ---------------------------------------------------------------------------
// /upload/:sessionId/:fileId
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UploadQuery {
    token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverProgressEvent {
    session_id: String,
    file_id: String,
    bytes_received: u64,
    total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileCompletedEvent {
    session_id: String,
    file_id: String,
    name: String,
    path: String,
    size: u64,
    verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionCompletedEvent {
    session_id: String,
    sender_alias: String,
    file_count: usize,
    total_size: u64,
    destination_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadProgress {
    bytes_received: u64,
    total: u64,
    completed: bool,
}

async fn handle_upload_progress(
    State(state): State<ServerState>,
    AxumPath((session_id, file_id)): AxumPath<(String, String)>,
    Query(q): Query<UploadQuery>,
) -> Result<Json<UploadProgress>, StatusCode> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.get(&session_id).ok_or(StatusCode::NOT_FOUND)?;
    let file = session.files.get(&file_id).ok_or(StatusCode::NOT_FOUND)?;
    if file.token != q.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(UploadProgress {
        bytes_received: file.bytes_received.load(Ordering::Relaxed),
        total: file.size,
        completed: file.completed.load(Ordering::Relaxed),
    }))
}

/// Parse a single "bytes=N-" Range header. Anything more elaborate
/// (multipart, suffix ranges) is ignored — we only support resume.
fn parse_range_start(headers: &HeaderMap) -> Option<u64> {
    let val = headers.get("range")?.to_str().ok()?;
    let rest = val.strip_prefix("bytes=")?;
    let dash = rest.find('-')?;
    let start_str = &rest[..dash];
    start_str.parse::<u64>().ok()
}

async fn handle_upload(
    State(state): State<ServerState>,
    AxumPath((session_id, file_id)): AxumPath<(String, String)>,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> StatusCode {
    // Look up the session + file.
    let (file, dest_dir, sender_alias) = {
        let sessions = state.sessions.lock().unwrap();
        let Some(session) = sessions.get(&session_id) else {
            return StatusCode::NOT_FOUND;
        };
        let Some(file) = session.files.get(&file_id) else {
            return StatusCode::NOT_FOUND;
        };
        if file.token != q.token {
            return StatusCode::UNAUTHORIZED;
        }
        (file.clone(), session.destination_dir.clone(), session.sender_alias.clone())
    };

    // Resolve target path safely.
    let Some(target_path) = safe_join(&dest_dir, &file.name, file.rel_path.as_deref()) else {
        eprintln!("[http] rejected unsafe path: {} / {:?}", file.name, file.rel_path);
        return StatusCode::BAD_REQUEST;
    };
    if let Some(parent) = target_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            eprintln!("[http] mkdir {} failed: {}", parent.display(), e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }

    // Resume support: if the client sends `Range: bytes=N-`, open the
    // file for read+write, seek to N, and resume. Otherwise truncate as
    // before. Hashing is disabled on resumed transfers because we
    // can't rebuild the prefix hash without re-reading what's on disk
    // (acceptable trade-off — final-size + per-file completion still
    // catches truncated transfers).
    let resume_from = parse_range_start(&headers).unwrap_or(0);
    let resumed = resume_from > 0;
    let mut handle = if resumed {
        match tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&target_path)
            .await
        {
            Ok(mut f) => {
                use tokio::io::AsyncSeekExt;
                if let Err(e) = f
                    .seek(std::io::SeekFrom::Start(resume_from))
                    .await
                {
                    eprintln!("[http] seek to {} failed: {}", resume_from, e);
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }
                file.bytes_received.store(resume_from, Ordering::Relaxed);
                eprintln!(
                    "[http] /upload resuming {} from byte {}",
                    file.name, resume_from
                );
                f
            }
            Err(e) => {
                eprintln!("[http] open for resume {} failed: {}", target_path.display(), e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    } else {
        match tokio::fs::File::create(&target_path).await {
            Ok(f) => {
                file.bytes_received.store(0, Ordering::Relaxed);
                f
            }
            Err(e) => {
                eprintln!("[http] create {} failed: {}", target_path.display(), e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    };

    // Hashing only when we sent the whole file in one shot.
    let mut hasher = (!resumed && file.sha256.is_some()).then(Sha256::new);
    let total = file.size;
    let mut stream = body.into_data_stream();
    let mut last_emit = Instant::now();
    let emit_every = Duration::from_millis(120);

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[http] body stream error: {}", e);
                return StatusCode::BAD_REQUEST;
            }
        };
        if let Err(e) = handle.write_all(&chunk).await {
            eprintln!("[http] write failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        if let Some(h) = hasher.as_mut() {
            h.update(&chunk);
        }
        let new_total = file.bytes_received.fetch_add(chunk.len() as u64, Ordering::Relaxed)
            + chunk.len() as u64;

        if last_emit.elapsed() > emit_every {
            let _ = state.app.emit(
                "transfer-progress-receiver",
                &ReceiverProgressEvent {
                    session_id: session_id.clone(),
                    file_id: file_id.clone(),
                    bytes_received: new_total,
                    total,
                },
            );
            last_emit = Instant::now();
        }
    }

    if let Err(e) = handle.flush().await {
        eprintln!("[http] flush failed: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    drop(handle);

    let verified = if let (Some(h), Some(expected)) = (hasher, file.sha256.as_ref()) {
        let got = hex::encode(h.finalize());
        if &got == expected {
            true
        } else {
            eprintln!("[http] hash mismatch: expected {} got {}", expected, got);
            // Leave file on disk but flag it
            false
        }
    } else {
        true
    };

    file.completed.store(true, Ordering::Relaxed);

    // ANDROID: copy the finished file to the *public* /Downloads/
    // folder via MediaStore so it's visible from Files / Gallery and
    // not stuck in the app-private sandbox. We keep streaming to the
    // app-scoped path for the actual transfer (so resume / hashing /
    // existing tokio::fs paths keep working unchanged), then publish
    // the final file in one shot. The app-scoped copy is removed
    // afterwards to avoid duplicates.
    #[cfg(target_os = "android")]
    {
        if let Ok(bytes) = tokio::fs::read(&target_path).await {
            let mime = file
                .rel_path
                .as_deref()
                .and_then(|p| mime_guess::from_path(p).first())
                .or_else(|| mime_guess::from_path(&file.name).first())
                .map(|m| m.essence_str().to_string());
            match crate::android_fs_bridge::save_to_public_downloads(
                &state.app,
                &file.name,
                &bytes,
                mime.as_deref(),
            )
            .await
            {
                Ok(public_uri) => {
                    eprintln!(
                        "[http] published {} to public Downloads: {}",
                        file.name, public_uri
                    );
                    // Remove the private copy now that the public one
                    // is in MediaStore.
                    let _ = tokio::fs::remove_file(&target_path).await;
                }
                Err(e) => {
                    eprintln!(
                        "[http] could not publish {} to /Downloads: {} (file stays at {})",
                        file.name,
                        e,
                        target_path.display()
                    );
                }
            }
        }
    }

    // Final progress event
    let _ = state.app.emit(
        "transfer-progress-receiver",
        &ReceiverProgressEvent {
            session_id: session_id.clone(),
            file_id: file_id.clone(),
            bytes_received: file.bytes_received.load(Ordering::Relaxed),
            total,
        },
    );

    let _ = state.app.emit(
        "file-completed",
        &FileCompletedEvent {
            session_id: session_id.clone(),
            file_id: file_id.clone(),
            name: file.name.clone(),
            path: target_path.to_string_lossy().to_string(),
            size: file.bytes_received.load(Ordering::Relaxed),
            verified,
        },
    );

    // If every file in the session is done, emit session-completed and clean up.
    let maybe_done = {
        let sessions = state.sessions.lock().unwrap();
        sessions.get(&session_id).map(|s| {
            (
                s.files.values().all(|f| f.completed.load(Ordering::Relaxed)),
                s.files.len(),
                s.total_size,
                s.destination_dir.clone(),
                s.sender_alias.clone(),
            )
        })
    };
    if let Some((all_done, count, total_size, dir, alias)) = maybe_done {
        if all_done {
            state.sessions.lock().unwrap().remove(&session_id);
            let _ = state.app.emit(
                "session-completed",
                &SessionCompletedEvent {
                    session_id: session_id.clone(),
                    sender_alias: alias.clone(),
                    file_count: count,
                    total_size,
                    destination_dir: dir.to_string_lossy().to_string(),
                },
            );
            println!(
                "[http] session {} from {} done ({} files, {} bytes)",
                session_id, alias, count, total_size
            );
        }
    }

    let _ = sender_alias; // silence unused-warning placeholder for future use
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// /cancel/:sessionId
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardPayload {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardReceivedEvent {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    received_at: i64,
}

async fn handle_clipboard(
    State(state): State<ServerState>,
    Json(payload): Json<ClipboardPayload>,
) -> StatusCode {
    // Mutual-consent check: we only accept clipboard pushes from peers
    // that we explicitly opted in to.
    if !state.clipboard.is_enabled(&payload.sender_fingerprint) {
        return StatusCode::FORBIDDEN;
    }

    let hash = hash_text(&payload.text);
    state.clipboard.note_synced(hash);

    // Write to the local clipboard. arboard handles desktop; Android
    // uses tauri-plugin-clipboard-manager because arboard doesn't
    // compile for that target.
    #[cfg(not(target_os = "android"))]
    let written = {
        let text_for_write = payload.text.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            cb.set_text(text_for_write).map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
    };
    #[cfg(target_os = "android")]
    let written: Result<Result<(), String>, tokio::task::JoinError> = {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        let result = state
            .app
            .clipboard()
            .write_text(payload.text.clone())
            .map_err(|e| e.to_string());
        Ok(result)
    };

    if let Ok(Err(e)) = written {
        eprintln!("[clipboard] failed to write OS clipboard: {}", e);
    }

    let _ = state.app.emit(
        "clipboard-received",
        &ClipboardReceivedEvent {
            text: payload.text,
            sender_alias: payload.sender_alias,
            sender_fingerprint: payload.sender_fingerprint,
            received_at: unix_now(),
        },
    );
    StatusCode::OK
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardImagePayload {
    /// Base64-encoded PNG (no data: prefix).
    png_base64: String,
    sender_alias: String,
    sender_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardImageReceivedEvent {
    sender_alias: String,
    sender_fingerprint: String,
    width: u32,
    height: u32,
    received_at: i64,
}

async fn handle_clipboard_image(
    State(state): State<ServerState>,
    Json(payload): Json<ClipboardImagePayload>,
) -> StatusCode {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    // Mutual-consent: same gate as text clipboard sync.
    if !state.clipboard.is_enabled(&payload.sender_fingerprint) {
        return StatusCode::FORBIDDEN;
    }
    let bytes = match B64.decode(payload.png_base64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }

    // Loop prevention shares the same hash channel as text — image bytes
    // are hashed instead, but `is_recent` / `note_synced` work the same.
    let hash = crate::clipboard_sync::hash_bytes(&bytes);
    state.clipboard.note_synced(hash);

    // Decode + rewrite to OS clipboard on a blocking thread. arboard
    // wants an ImageData with RGBA8 pixels, so we go PNG → DynamicImage
    // → RGBA8 raw. On Android arboard isn't available; we only return
    // the WxH for the UI event (the OS clipboard write comes later).
    #[cfg(not(target_os = "android"))]
    let decoded = tokio::task::spawn_blocking(move || -> Result<(u32, u32), String> {
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let image_data = arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(rgba.into_raw()),
        };
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        cb.set_image(image_data).map_err(|e| e.to_string())?;
        Ok((w, h))
    })
    .await;
    // Android: writing arbitrary images to the system clipboard
    // requires a FileProvider + JNI bridge to ClipboardManager
    // (v0.14.0 territory). What we CAN do today, thanks to
    // tauri-plugin-android-fs, is drop the PNG into the public
    // Pictures/Millennium/ MediaStore entry and ask the OS to scan it
    // — Gallery / Photos see it instantly, the user just opens it from
    // there. The whole thing happens off the http handler thread on
    // purpose because the plugin internals do real Android JNI work.
    #[cfg(target_os = "android")]
    let decoded: Result<Result<(u32, u32), String>, tokio::task::JoinError> = {
        let bytes_for_save = bytes.clone();
        let app_handle = state.app.clone();
        let inner: Result<(u32, u32), String> = async move {
            let img = image::load_from_memory(&bytes_for_save)
                .map_err(|e| format!("decode png: {e}"))?;
            let (w, h) = (img.width(), img.height());
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let filename = format!("clipboard_{}.png", ts);
            match crate::android_fs_bridge::save_image_to_gallery(
                &app_handle,
                &filename,
                &bytes_for_save,
            )
            .await
            {
                Ok(uri) => {
                    eprintln!("[clipboard] saved incoming image to {uri}");
                }
                Err(e) => {
                    eprintln!("[clipboard] save_image_to_gallery failed: {e}");
                }
            }
            Ok((w, h))
        }
        .await;
        Ok(inner)
    };

    let (width, height) = match decoded {
        Ok(Ok(dims)) => dims,
        Ok(Err(e)) => {
            eprintln!("[clipboard] failed to write image clipboard: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        Err(e) => {
            eprintln!("[clipboard] blocking task crashed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let _ = state.app.emit(
        "clipboard-image-received",
        &ClipboardImageReceivedEvent {
            sender_alias: payload.sender_alias,
            sender_fingerprint: payload.sender_fingerprint,
            width,
            height,
            received_at: unix_now(),
        },
    );
    StatusCode::OK
}

async fn handle_cancel(
    State(state): State<ServerState>,
    AxumPath(session_id): AxumPath<String>,
) -> StatusCode {
    let removed = state.sessions.lock().unwrap().remove(&session_id).is_some();
    // Also drop pending approval if any
    let _ = approval_registry().lock().unwrap().remove(&session_id);
    if removed {
        let _ = state.app.emit(
            "session-cancelled",
            serde_json::json!({ "sessionId": session_id }),
        );
        println!("[http] /cancel {} removed", session_id);
    }
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// True si el componente es un nombre de archivo seguro para escribir en
/// disco (multiplataforma, con foco en las trampas de Windows). Rechaza
/// dispositivos reservados (CON/NUL/COM1..), ADS (`:`), caracteres
/// ilegales en NTFS, bytes de control y dots/espacios finales.
fn is_safe_component(s: &std::ffi::OsStr) -> bool {
    let name = match s.to_str() {
        Some(n) => n,
        None => return false, // no UTF-8 válido → rechazar
    };
    if name.is_empty() {
        return false;
    }
    // ADS / drive-relative: cualquier ':' es sospechoso en Windows.
    if name.contains(':') {
        return false;
    }
    // Caracteres ilegales en NTFS (y peligrosos en general).
    if name.contains(['/', '\\', '<', '>', '"', '|', '?', '*']) {
        return false;
    }
    // Bytes de control.
    if name.chars().any(|c| (c as u32) < 0x20) {
        return false;
    }
    // Windows strippea '.' y espacios finales → colisión/escritura en
    // un nombre distinto del pedido.
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    // Nombres de dispositivo reservados (con o sin extensión):
    // CON, PRN, AUX, NUL, COM1..COM9, LPT1..LPT9.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return false;
    }
    true
}

/// Join base + optional relative folder + filename, refusing anything
/// that escapes via `..`, absolute components or Windows drive prefixes,
/// and any component that `is_safe_component` rejects (reserved device
/// names, ADS, trailing dots/spaces, illegal chars).
fn safe_join(base: &Path, name: &str, rel_path: Option<&str>) -> Option<PathBuf> {
    let mut target = base.to_path_buf();
    if let Some(rel) = rel_path {
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(s) if is_safe_component(s) => target.push(s),
                _ => return None,
            }
        }
    }
    for comp in Path::new(name).components() {
        match comp {
            Component::Normal(s) if is_safe_component(s) => target.push(s),
            _ => return None,
        }
    }
    Some(target)
}

#[cfg(all(test, not(windows)))]
mod safe_join_tests {
    use super::*;

    fn base() -> PathBuf {
        PathBuf::from("/tmp/dl")
    }

    #[test]
    fn rejects_reserved_device_names() {
        assert!(safe_join(&base(), "CON", None).is_none());
        assert!(safe_join(&base(), "con", None).is_none()); // case-insensitive
        assert!(safe_join(&base(), "NUL.txt", None).is_none()); // stem reservado
        assert!(safe_join(&base(), "COM1", None).is_none());
        assert!(safe_join(&base(), "LPT9.log", None).is_none());
        assert!(safe_join(&base(), "PRN", None).is_none());
        assert!(safe_join(&base(), "AUX", None).is_none());
    }

    #[test]
    fn rejects_ads_and_illegal_chars() {
        assert!(safe_join(&base(), "a:b", None).is_none()); // ADS
        assert!(safe_join(&base(), "report.txt:hidden", None).is_none());
        assert!(safe_join(&base(), "foo<bar", None).is_none());
        assert!(safe_join(&base(), "pipe|name", None).is_none());
    }

    #[test]
    fn rejects_trailing_dot_or_space() {
        assert!(safe_join(&base(), "trailing.", None).is_none());
        assert!(safe_join(&base(), "trailing ", None).is_none());
    }

    #[test]
    fn accepts_legit_names_with_internal_dots() {
        assert!(safe_join(&base(), "report.final.pdf", None).is_some());
        assert!(safe_join(&base(), "normal.txt", None).is_some());
        assert!(safe_join(&base(), "CONELRAD.txt", None).is_some()); // no es CON
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        assert!(safe_join(&base(), "../escape", None).is_none());
        assert!(safe_join(&base(), "/etc/passwd", None).is_none());
    }

    #[test]
    fn rejects_reserved_subdir_in_rel_path() {
        assert!(safe_join(&base(), "ok.txt", Some("sub/NUL")).is_none());
        assert!(safe_join(&base(), "ok.txt", Some("good/dir")).is_some());
    }
}
