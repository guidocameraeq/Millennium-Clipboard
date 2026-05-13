// Millennium Clipboard — HTTPS server (Fase 4)
//
// axum + axum-server + rustls. Currently exposes only `/info` so peers
// can confirm identity after mDNS resolution. Transfer endpoints land
// in Fase 5 (POST /text) and Fase 7 (POST /upload).

use anyhow::{Context, Result};
use axum::{extract::State, routing::get, Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoResponse {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub version: String,
    pub protocol: String,
}

pub async fn run(
    port: u16,
    info: InfoResponse,
    cert_pem: String,
    key_pem: String,
) -> Result<()> {
    let state = Arc::new(info);
    let app = Router::new()
        .route("/info", get(handle_info))
        .with_state(state);

    let tls = RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await
        .context("load TLS config")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("[http] HTTPS server listening on {}", addr);

    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await
        .context("axum server")?;

    Ok(())
}

async fn handle_info(State(info): State<Arc<InfoResponse>>) -> Json<InfoResponse> {
    Json((*info).clone())
}
