// Millennium Clipboard — HTTPS client (Fase 5–7, cert pinning Fase 3)
//
// Peers are self-signed and identify themselves by the SHA-256 of their
// DER cert (their "fingerprint"). We do NOT validate a CA chain or CN/SAN.
// Instead the TLS handshake is PINNED: the client only completes the
// handshake if the server's end-entity cert hashes to the fingerprint we
// expect (see PinnedFingerprintVerifier). This closes the old MITM hole
// where a spoofable /info probe "verified" the peer on a different socket
// than the one carrying the payload.
//
// TOFU mode (expected == None) accepts any cert and is used ONLY for
// discovery/pairing (fetch_info, add_peer_by_ip, pair_with_qr_payload),
// where we don't trust any fingerprint yet — the real pin happens on the
// NEXT send to that peer.
//
// IMPORTANT: Clients are pooled PER expected-fingerprint (a cached
// HashMap<fp, Client>), because use_preconfigured_tls freezes the verifier
// at build() time so we need one Client per pin. reqwest::Client is a cheap
// Arc clone that shares the pool, so cloning per request does NOT reopen
// connections — this keeps the pooling that avoids LocalSend bug #1657
// (7000 small files collapsing to 80 KB/s when each upload pays a fresh
// TLS handshake).

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::io::ReaderStream;

const CHUNK: usize = 64 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

// ---------------------------------------------------------------------------
// Cert pinning (Fase 3, Tarea 3.1)
// ---------------------------------------------------------------------------

/// Verifier that pins the SHA-256 fingerprint of the end-entity cert.
/// If `expected` is None it runs in TOFU mode: accepts any cert (equivalent
/// to the old behaviour) — used ONLY for /info during discovery/pairing
/// where we don't trust anyone yet.
#[derive(Debug)]
struct PinnedFingerprintVerifier {
    expected: Option<String>, // hex lowercase, same definition as identity.fingerprint
}

/// The ring provider's signature-verification algorithms, computed once.
/// Used to actually verify the handshake signature (see below).
fn ring_sig_algs() -> &'static rustls::crypto::WebPkiSupportedAlgorithms {
    static ALGS: OnceLock<rustls::crypto::WebPkiSupportedAlgorithms> = OnceLock::new();
    ALGS.get_or_init(|| rustls::crypto::ring::default_provider().signature_verification_algorithms)
}

impl ServerCertVerifier for PinnedFingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let mut hasher = Sha256::new();
        hasher.update(end_entity.as_ref());
        let got = hex::encode(hasher.finalize());
        match &self.expected {
            None => Ok(ServerCertVerified::assertion()), // TOFU
            Some(exp) if exp.eq_ignore_ascii_case(&got) => Ok(ServerCertVerified::assertion()),
            Some(exp) => Err(TlsError::General(format!(
                "cert fingerprint mismatch: expected {}, got {}",
                &exp[..16.min(exp.len())],
                &got[..16.min(got.len())]
            ))),
        }
    }

    // We deliberately skip CA-chain / CN / SAN validation (peers are
    // self-signed and identified by fingerprint, not hostname) — the pin in
    // verify_server_cert replaces that. But we MUST still verify the handshake
    // signature: it is the ONLY proof that the peer holds the PRIVATE key of
    // the presented cert. Skipping it (returning assertion()) would let an
    // attacker present a COPIED, public cert that hashes to the pinned
    // fingerprint and MITM without owning the key. So delegate these two hooks
    // to rustls' real signature verifiers (ring provider).
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, ring_sig_algs())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, ring_sig_algs())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        ring_sig_algs().supported_schemes()
    }
}

fn build_client(expected_fp: Option<&str>) -> reqwest::Client {
    let verifier = Arc::new(PinnedFingerprintVerifier {
        expected: expected_fp.map(|s| s.to_ascii_lowercase()),
    });

    let mut cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    // ALPN empty is fine: axum-server doesn't negotiate h2 here.
    cfg.alpn_protocols.clear();

    reqwest::Client::builder()
        .use_preconfigured_tls(cfg)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .timeout(Duration::from_secs(300))
        .build()
        .expect("build reqwest client")
}

/// Return a client pinned to `expected_fp`, created once and cached. Keyed by
/// the fingerprint (or "__tofu__" for the discovery mode). The Mutex is held
/// only long enough to clone the (Arc-backed) Client out — never across an
/// await — so it respects the no-lock-across-await rule.
fn client_for(expected_fp: Option<&str>) -> reqwest::Client {
    static CACHE: OnceLock<Mutex<HashMap<String, reqwest::Client>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = expected_fp
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "__tofu__".to_string());
    let mut map = cache.lock().unwrap();
    map.entry(key)
        .or_insert_with(|| build_client(expected_fp))
        .clone()
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

/// TOFU probe (no pinning) — for discovery/pairing where we don't trust a
/// fingerprint yet (add_peer_by_ip, pair_with_qr_payload, self-ping). The
/// caller compares the returned fingerprint itself.
pub async fn fetch_info(ip: &str, port: u16) -> Result<RemoteInfo> {
    fetch_info_inner(ip, port, None).await
}

/// Pinned probe — for the discovery poller, which already knows the peer's
/// expected fingerprint. The TLS handshake fails if the cert at (ip, port)
/// doesn't hash to `expected_fp`, so an impostor squatting the address can't
/// keep the peer marked "online".
pub async fn fetch_info_pinned(ip: &str, port: u16, expected_fp: &str) -> Result<RemoteInfo> {
    fetch_info_inner(ip, port, Some(expected_fp)).await
}

async fn fetch_info_inner(ip: &str, port: u16, expected_fp: Option<&str>) -> Result<RemoteInfo> {
    let url = format!("https://{}:{}/info", ip, port);
    let resp = client_for(expected_fp)
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
    expected_fp: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/text", ip, port);
    let resp = client_for(Some(expected_fp))
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

#[allow(clippy::too_many_arguments)]
pub async fn prepare_upload(
    ip: &str,
    port: u16,
    session_id: &str,
    sender_alias: &str,
    sender_fingerprint: &str,
    sender_port: u16,
    files: &[PrepareFile],
    expected_fp: &str,
) -> Result<PrepareUploadResponse> {
    let url = format!("https://{}:{}/prepare-upload", ip, port);
    let resp = client_for(Some(expected_fp))
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
    expected_fp: &str,
) -> u64 {
    let url = format!(
        "https://{}:{}/upload/{}/{}/progress?token={}",
        ip, port, session_id, file_id, token
    );
    match client_for(Some(expected_fp)).get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<UploadProgress>().await {
            Ok(p) => p.bytes_received,
            Err(_) => 0,
        },
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn upload_file(
    app: AppHandle,
    ip: &str,
    port: u16,
    session_id: &str,
    file_id: &str,
    token: &str,
    path: &Path,
    total: u64,
    expected_fp: &str,
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
            fetch_upload_progress(ip, port, session_id, file_id, token, expected_fp).await
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
            expected_fp,
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

#[allow(clippy::too_many_arguments)]
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
    expected_fp: &str,
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
    let mut req = client_for(Some(expected_fp))
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

pub async fn cancel_upload(
    ip: &str,
    port: u16,
    session_id: &str,
    expected_fp: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/cancel/{}", ip, port, session_id);
    let _ = client_for(Some(expected_fp)).post(&url).send().await; // best-effort
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
    receiver_fp: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/clipboard", ip, port);
    let resp = client_for(Some(receiver_fp))
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
    receiver_fp: &str,
) -> Result<()> {
    let url = format!("https://{}:{}/clipboard/image", ip, port);
    let resp = client_for(Some(receiver_fp))
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

// ---------------------------------------------------------------------------
// Tests — cert pinning verifier (Fase 3, Tarea 3.1)
// ---------------------------------------------------------------------------
// Gated not(windows): adding any test to the crate breaks the lib test binary
// load on Windows (comctl32-v6 / STATUS_ENTRYPOINT_NOT_FOUND). Verified in an
// isolated cargo harness on the host; runs normally on non-Windows CI.
#[cfg(all(test, not(windows)))]
mod pinning_tests {
    use super::*;

    /// Generate a real self-signed cert (same crate as identity.rs) and its
    /// SHA-256 DER fingerprint — the exact definition the app pins against.
    fn make_cert_der() -> (CertificateDer<'static>, String) {
        let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let der = ck.cert.der().clone();
        let mut h = Sha256::new();
        h.update(der.as_ref());
        let fp = hex::encode(h.finalize());
        (der, fp)
    }

    fn verify(v: &PinnedFingerprintVerifier, der: &CertificateDer<'_>) -> Result<(), TlsError> {
        let name = ServerName::try_from("localhost").unwrap();
        v.verify_server_cert(der, &[], &name, &[], UnixTime::now())
            .map(|_| ())
    }

    #[test]
    fn accepts_matching_fingerprint() {
        let (der, fp) = make_cert_der();
        let v = PinnedFingerprintVerifier { expected: Some(fp) };
        assert!(verify(&v, &der).is_ok());
    }

    #[test]
    fn accepts_matching_fingerprint_case_insensitive() {
        let (der, fp) = make_cert_der();
        let v = PinnedFingerprintVerifier { expected: Some(fp.to_uppercase()) };
        assert!(verify(&v, &der).is_ok());
    }

    #[test]
    fn rejects_mismatched_fingerprint() {
        let (der, _fp) = make_cert_der();
        let v = PinnedFingerprintVerifier { expected: Some("a".repeat(64)) };
        assert!(verify(&v, &der).is_err());
    }

    #[test]
    fn tofu_accepts_any_cert() {
        let (der, _fp) = make_cert_der();
        let v = PinnedFingerprintVerifier { expected: None };
        assert!(verify(&v, &der).is_ok());
    }
}
