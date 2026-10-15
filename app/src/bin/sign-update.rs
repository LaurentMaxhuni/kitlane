//! Maintainer tool: keeps local signing seed in the OS credential vault.
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use kitlane_sync_client::{OsVault, Vault};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const ACCOUNT: &str = "kitlane.release-signing.v1";

fn key() -> Result<SigningKey, String> {
    let encoded = Zeroizing::new(match std::env::var("KITLANE_UPDATE_SIGNING_KEY") {
        Ok(value) => value,
        Err(_) => OsVault
            .get(ACCOUNT)
            .map_err(|_| "Signing credential vault unavailable")?
            .ok_or("Signing key absent; run sign-update keygen")?,
    });
    let bytes = Zeroizing::new(
        STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| "Signing seed must be base64")?,
    );
    let seed: Zeroizing<[u8; 32]> = Zeroizing::new(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Signing seed must contain exactly 32 bytes")?,
    );
    Ok(SigningKey::from_bytes(&seed))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") if args.len() == 1 => {
            if OsVault.get(ACCOUNT).map_err(|_| "Signing credential vault unavailable")?.is_some() { return Err("Signing key already exists; refusing replacement".into()); }
            let mut seed = Zeroizing::new([0u8; 32]);
            getrandom::fill(seed.as_mut()).map_err(|_| "Secure random generator unavailable")?;
            let signer = SigningKey::from_bytes(&seed);
            let encoded = Zeroizing::new(STANDARD.encode(seed.as_ref()));
            OsVault.set(ACCOUNT, &encoded).map_err(|_| "Cannot save signing key in credential vault")?;
            println!("{}", serde_json::json!({"id":"kitlane-release-2026-01","public_key":STANDARD.encode(signer.verifying_key().as_bytes())}));
        }
        Some("public") if args.len() == 1 => {
            println!("{}", STANDARD.encode(key()?.verifying_key().as_bytes()));
        }
        Some("sign") if args.len() == 5 => {
            let installer = PathBuf::from(&args[1]);
            let version = semver::Version::parse(&args[2]).map_err(|_| "Invalid version")?;
            if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != args[2] { return Err("Version must be a canonical numeric release version".into()); }
            let sequence: u64 = args[3].parse().map_err(|_| "Invalid sequence")?;
            if sequence == 0 { return Err("Sequence must be positive".into()); }
            let bytes = fs::read(&installer).map_err(|e| e.to_string())?;
            if bytes.is_empty() || bytes.len() > 512 * 1024 * 1024 || installer.file_name().and_then(|n| n.to_str()) != Some(format!("Kitlane-{version}-Setup-x64.exe").as_str()) { return Err("Expected bounded official Windows x64 installer filename".into()); }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
            let payload = serde_json::to_vec(&serde_json::json!({"schema":1,"repository":"LaurentMaxhuni/kitlane","channel":"preview","sequence":sequence,"version":version.to_string(),"published_at":now,"expires_at":now+30*24*60*60,"os":"windows","arch":"x86_64","size":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))})).map_err(|e| e.to_string())?;
            let key = key()?;
            let root: serde_json::Value = serde_json::from_str(include_str!("../../update-root.json")).map_err(|e| e.to_string())?;
            let public = STANDARD.encode(key.verifying_key().as_bytes());
            let key_entry = root["keys"].as_array().and_then(|keys| keys.iter().find(|entry| entry["public_key"].as_str() == Some(&public))).ok_or("Signing public key is absent from bundled update root")?;
            let id = key_entry["id"].as_str().ok_or("Invalid root key id")?;
            if root["revoked_key_ids"].as_array().is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(id))) { return Err("Signing key is revoked".into()); }
            let signed = [b"kitlane-update-v1\0".as_slice(), payload.as_slice()].concat();
            let envelope = serde_json::json!({"key_id":id,"payload":STANDARD.encode(payload),"signature":STANDARD.encode(key.sign(&signed).to_bytes())});
            fs::write(&args[4], serde_json::to_vec_pretty(&envelope).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            println!("Signed Kitlane {version}, sequence {sequence}");
        }
        _ => return Err("Usage: sign-update keygen | public | sign <installer> <version> <sequence> <output.json>".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
