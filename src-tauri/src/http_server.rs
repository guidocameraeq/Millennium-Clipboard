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
    http::StatusCode,
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

pub async fn run(
    app: AppHandle,
    port: u16,
    info: InfoResponse,
    cert_pem: String,
    key_pem: String,
    prefs: Arc<PreferencesStore>,
    settings: Arc<SettingsStore>,
) -> Result<()> {
    let state = ServerState {
        info: Arc::new(info),
        app,
        prefs,
        settings,
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    let router = Router::new()
        .route("/info", get(handle_info))
        .route("/text", post(handle_text))
        .route("/prepare-upload", post(handle_prepare_upload))
        .route("/upload/{session_id}/{file_id}", post(handle_upload))
        .route("/cancel/{session_id}", post(handle_cancel))
        .with_state(state);

    let tls = RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await
        .context("load TLS config")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("[http] HTTPS server listening on {}", addr);

    axum_server::bind_rustls(addr, tls)
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

async fn handle_upload(
    State(state): State<ServerState>,
    AxumPath((session_id, file_id)): AxumPath<(String, String)>,
    Query(q): Query<UploadQuery>,
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

    let mut handle = match tokio::fs::File::create(&target_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[http] create {} failed: {}", target_path.display(), e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let mut hasher = file.sha256.is_some().then(Sha256::new);
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

/// Join base + optional relative folder + filename, refusing anything
/// that escapes via `..`, absolute components or Windows drive prefixes.
fn safe_join(base: &Path, name: &str, rel_path: Option<&str>) -> Option<PathBuf> {
    let mut target = base.to_path_buf();
    if let Some(rel) = rel_path {
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(s) => target.push(s),
                _ => return None,
            }
        }
    }
    for comp in Path::new(name).components() {
        match comp {
            Component::Normal(s) => target.push(s),
            _ => return None,
        }
    }
    Some(target)
}
