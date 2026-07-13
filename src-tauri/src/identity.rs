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
use std::net::{IpAddr, Ipv4Addr};
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
        // Dev convenience: spawning a second instance on the same box
        // would otherwise share identity.json. MILLENNIUM_INSTANCE=N
        // gives that instance its own identity-N.json so two peers
        // can find each other locally.
        let filename = match std::env::var("MILLENNIUM_INSTANCE").ok() {
            Some(s) if !s.is_empty() => format!("identity-{}.json", s),
            _ => "identity.json".to_string(),
        };
        let identity_file = data_dir.join(filename);

        if identity_file.exists() {
            let raw = fs::read_to_string(&identity_file)
                .with_context(|| format!("read {}", identity_file.display()))?;
            let mut id: Identity = serde_json::from_str(&raw)
                .with_context(|| format!("parse {}", identity_file.display()))?;
            id.local_ip = compute_local_ip();
            crate::runtime_log::info(format!(
                "[identity] loaded existing identity hex_id={} fp={}",
                id.hex_id,
                &id.fingerprint[..16.min(id.fingerprint.len())]
            ));
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
        crate::runtime_log::info(format!(
            "[identity] generated NEW identity hex_id={} fp={} saved to {}",
            identity.hex_id,
            &identity.fingerprint[..16.min(identity.fingerprint.len())],
            identity_file.display()
        ));

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

/// Adapter names that are almost always virtual / tunnel NICs. WSL and
/// Hyper-V expose "vEthernet (...)", VPNs surface as "VPN"/"tun"/"tap",
/// etc. Deliberately conservative: a false negative just risks announcing
/// on a virtual NIC (the old behaviour), while a false positive could
/// discard the one real adapter — so we keep the list tight and fall back
/// to the routing-table IP when nothing clean survives the filter.
fn is_virtual_iface(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("vethernet")
        || n.contains("wsl")
        || n.contains("hyper-v")
        || n.contains("virtualbox")
        || n.contains("vmware")
        || n.contains("vpn")
        || n.contains("tailscale")
        || n.contains("zerotier")
        || n.contains("docker")
        || n.contains("loopback")
        || n.contains("tun")
        || n.contains("tap")
}

/// Pick the best local IPv4 from a list of (interface name, ip): the first
/// private, non-loopback, non-virtual address. Pure — no OS access — so it
/// can be unit-tested. Returns None when nothing clean is found (the caller
/// then falls back to the routing-table IP).
fn pick_local_ipv4(ifaces: &[(String, IpAddr)]) -> Option<Ipv4Addr> {
    ifaces.iter().find_map(|(name, ip)| match ip {
        IpAddr::V4(v4)
            if v4.is_private() && !v4.is_loopback() && !is_virtual_iface(name) =>
        {
            Some(*v4)
        }
        _ => None,
    })
}

/// Routing-table IP (lowest-metric route). Often a virtual NIC on machines
/// with WSL/Hyper-V/VPN — used only as a last resort.
fn fallback_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}

/// Resolve the address we announce on. Prefers a real physical LAN NIC over
/// the routing-table default (which frequently points at WSL/Hyper-V). `pub`
/// so discovery can recompute it after a network change.
pub fn compute_local_ip() -> String {
    match local_ip_address::list_afinet_netifas() {
        Ok(ifaces) => pick_local_ipv4(&ifaces)
            .map(|ip| ip.to_string())
            .unwrap_or_else(fallback_local_ip),
        Err(_) => fallback_local_ip(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn picks_physical_private_over_virtual_and_loopback() {
        let ifaces = vec![
            ("vEthernet (WSL)".to_string(), v4(172, 20, 0, 1)),
            ("Wi-Fi".to_string(), v4(192, 168, 1, 42)),
            ("Loopback Pseudo-Interface 1".to_string(), v4(127, 0, 0, 1)),
        ];
        assert_eq!(pick_local_ipv4(&ifaces), Some(Ipv4Addr::new(192, 168, 1, 42)));
    }

    #[test]
    fn skips_apipa_and_public_addresses() {
        // 169.254 (APIPA/link-local) and a public IP are not "private", so
        // neither is chosen; the real private LAN address is.
        let ifaces = vec![
            ("Ethernet".to_string(), v4(169, 254, 3, 3)),
            ("Ethernet 2".to_string(), v4(8, 8, 8, 8)),
            ("Ethernet 3".to_string(), v4(10, 0, 5, 9)),
        ];
        assert_eq!(pick_local_ipv4(&ifaces), Some(Ipv4Addr::new(10, 0, 5, 9)));
    }

    #[test]
    fn none_when_only_virtual_or_loopback() {
        let ifaces = vec![
            ("vEthernet (Default Switch)".to_string(), v4(172, 20, 0, 1)),
            ("lo".to_string(), v4(127, 0, 0, 1)),
        ];
        assert_eq!(pick_local_ipv4(&ifaces), None);
    }
}
