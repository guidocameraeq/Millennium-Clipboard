// Millennium Clipboard — HTTPS client (Fase 5)
//
// We talk to peers over HTTPS with self-signed certs. Cert chain
// validation is intentionally skipped — instead the caller compares the
// peer-reported fingerprint against the one we stored when mDNS first
// announced the peer. That cross-check lives in `lib.rs::send_text`.
// Future hardening (Fase 8+): pin the actual TLS cert SHA-256 via a
// rustls custom verifier.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub alias: String,
    pub fingerprint: String,
    pub hex_id: String,
    pub version: String,
    pub protocol: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TextPayload {
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")
}

pub async fn fetch_info(ip: &str, port: u16) -> Result<RemoteInfo> {
    let url = format!("https://{}:{}/info", ip, port);
    let resp = build_client()?
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        bail!("info endpoint returned {}", resp.status());
    }
    resp.json::<RemoteInfo>().await.context("decode /info JSON")
}

pub async fn post_text(
    ip: &str,
    port: u16,
    text: String,
    sender_alias: String,
    sender_fingerprint: String,
) -> Result<()> {
    let url = format!("https://{}:{}/text", ip, port);
    let resp = build_client()?
        .post(&url)
        .json(&TextPayload { text, sender_alias, sender_fingerprint })
        .send()
        .await
        .with_context(|| format!("POST {}", url))?;
    if !resp.status().is_success() {
        bail!("text endpoint returned {}", resp.status());
    }
    Ok(())
}
