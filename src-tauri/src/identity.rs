// Millennium Clipboard — identity (Fase 4)
//
// First-run generates a self-signed TLS cert and persists it. The
// SHA-256 of the DER cert becomes the stable peer fingerprint and the
// short hex_id used in UI. Subsequent runs load the same identity, so
// other peers always see the same id.

use anyhow::{Context, Result};
use rcgen::{generate_simple_self_signed, CertifiedKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub alias: String,
    pub fingerprint: String, // sha256 hex of DER cert
    pub hex_id: String,      // short pretty id, e.g. "0xAB:CD:EF"
    pub cert_pem: String,
    pub key_pem: String,

    // Not persisted — computed each run.
    #[serde(skip)]
    pub local_ip: String,
}

impl Identity {
    pub fn load_or_generate(data_dir: &Path) -> Result<Self> {
        let identity_file = data_dir.join("identity.json");

        if identity_file.exists() {
            let raw = fs::read_to_string(&identity_file)
                .with_context(|| format!("read {}", identity_file.display()))?;
            let mut id: Identity = serde_json::from_str(&raw)
                .with_context(|| format!("parse {}", identity_file.display()))?;
            id.local_ip = compute_local_ip();
            println!("[identity] loaded existing identity hex_id={}", id.hex_id);
            return Ok(id);
        }

        // First run: generate.
        let alias = compute_alias();
        let sans = vec![alias.clone(), "localhost".to_string()];

        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(sans).context("generate self-signed cert")?;
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // SHA-256 of the DER cert is our stable peer id.
        let der = cert.der();
        let mut hasher = Sha256::new();
        hasher.update(der.as_ref());
        let fingerprint = hex::encode(hasher.finalize());
        let hex_id = format_hex_id(&fingerprint);

        let identity = Identity {
            alias,
            fingerprint,
            hex_id,
            cert_pem,
            key_pem,
            local_ip: compute_local_ip(),
        };

        fs::create_dir_all(data_dir)
            .with_context(|| format!("mkdir {}", data_dir.display()))?;
        let json = serde_json::to_string_pretty(&identity).context("serialize identity")?;
        fs::write(&identity_file, json)
            .with_context(|| format!("write {}", identity_file.display()))?;
        println!(
            "[identity] generated new identity hex_id={} saved to {}",
            identity.hex_id,
            identity_file.display()
        );

        Ok(identity)
    }
}

fn format_hex_id(fingerprint: &str) -> String {
    // first 6 hex chars → "0xAB:CD:EF"
    let chars: Vec<&str> = (0..6).step_by(2).map(|i| &fingerprint[i..i + 2]).collect();
    format!("0x{}", chars.join(":").to_uppercase())
}

fn compute_alias() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "MILLENNIUM-NODE".into())
        .to_uppercase()
}

fn compute_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}
