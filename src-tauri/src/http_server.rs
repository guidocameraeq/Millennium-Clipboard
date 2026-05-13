// Millennium Clipboard — HTTPS server (Fase 4 + Fase 5)
//
// Routes:
//   GET  /info  → identity probe (Fase 4)
//   POST /text  → receive a text payload, emit `incoming-text` to UI (Fase 5)
//
// Transfer endpoints for files arrive in Fase 7.

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub version: String,
    pub protocol: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextPayload {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingText {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
    received_at: i64,
}

#[derive(Clone)]
struct ServerState {
    info: Arc<InfoResponse>,
    app: AppHandle,
}

pub async fn run(
    app: AppHandle,
    port: u16,
    info: InfoResponse,
    cert_pem: String,
    key_pem: String,
) -> Result<()> {
    let state = ServerState {
        info: Arc::new(info),
        app,
    };

    let router = Router::new()
        .route("/info", get(handle_info))
        .route("/text", post(handle_text))
        .with_state(state);

    let tls = RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await
        .context("load TLS config")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("[http] HTTPS server listening on {}", addr);

    axum_server::bind_rustls(addr, tls)
        .serve(router.into_make_service())
        .await
        .context("axum server")?;

    Ok(())
}

async fn handle_info(State(state): State<ServerState>) -> Json<InfoResponse> {
    Json((*state.info).clone())
}

async fn handle_text(
    State(state): State<ServerState>,
    Json(payload): Json<TextPayload>,
) -> StatusCode {
    let incoming = IncomingText {
        text: payload.text,
        sender_alias: payload.sender_alias,
        sender_fingerprint: payload.sender_fingerprint,
        received_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    };
    println!(
        "[http] /text received {} chars from {}",
        incoming.text.chars().count(),
        incoming.sender_alias
    );
    let _ = state.app.emit("incoming-text", &incoming);
    StatusCode::OK
}
