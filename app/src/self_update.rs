//! Signed official-release updates. Network data never controls commands or destinations.

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const REPOSITORY: &str = "LaurentMaxhuni/kitlane";
const CHANNEL: &str = "preview";
const METADATA_NAME: &str = "update-preview-v1.json";
const DOMAIN: &[u8] = b"kitlane-update-v1\0";
const MAX_METADATA: u64 = 64 * 1024;
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;
const MAX_LIFETIME: u64 = 31 * 24 * 60 * 60;

#[derive(Debug)]
pub enum CheckResult {
    Unsupported(String),
    NotConfigured(String),
    Current,
    Available(Box<VerifiedUpdate>),
}

#[derive(Clone, Debug)]
pub struct VerifiedUpdate {
    manifest: Manifest,
    envelope: Vec<u8>,
    digest: String,
}
impl VerifiedUpdate {
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}

#[derive(Debug)]
pub struct StagedUpdate {
    update: VerifiedUpdate,
    path: PathBuf,
    state_dir: PathBuf,
}
impl StagedUpdate {
    pub fn version(&self) -> &str {
        self.update.version()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    schema: u32,
    minimum_sequence: u64,
    keys: Vec<RootKey>,
    revoked_key_ids: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootKey {
    id: String,
    public_key: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    key_id: String,
    payload: String,
    signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    repository: String,
    channel: String,
    sequence: u64,
    version: String,
    published_at: u64,
    expires_at: u64,
    os: String,
    arch: String,
    size: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    sequence: u64,
    digest: String,
}

fn signed_bytes(payload: &[u8]) -> Vec<u8> {
    [DOMAIN, payload].concat()
}
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(|_| "System clock predates Unix epoch".into())
}
fn root() -> Result<Root, String> {
    serde_json::from_str(include_str!("../update-root.json"))
        .map_err(|_| "Invalid bundled update trust root".into())
}

#[allow(dead_code)] // UI status helper; the standalone agent uses check() directly.
pub fn signing_configured() -> bool {
    root().is_ok_and(|root| !root.keys.is_empty())
}
fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read update: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err("Update exceeds size limit".into());
    }
    Ok(bytes)
}
fn read_receipt(dir: &Path) -> Result<Option<Receipt>, String> {
    require_plain_path(&dir.join("accepted-update.json"))?;
    let file = match File::open(dir.join("accepted-update.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read update history: {e}")),
    };
    serde_json::from_slice(&read_bounded(file, 4096)?)
        .map(Some)
        .map_err(|_| "Update history is corrupt; refusing rollback".into())
}

fn record_accepted(dir: &Path, update: &VerifiedUpdate) -> Result<(), String> {
    require_plain_path(&dir.join("accepted-update.json"))?;
    fs::create_dir_all(dir).map_err(|e| format!("Cannot create updater directory: {e}"))?;
    let receipt = Receipt {
        sequence: update.manifest.sequence,
        digest: update.digest.clone(),
    };
    // A crash during this write fails closed on the next check.
    let mut history = File::create(dir.join("accepted-update.json")).map_err(|e| e.to_string())?;
    history
        .write_all(&serde_json::to_vec(&receipt).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    history.sync_all().map_err(|e| e.to_string())
}

// Refuse links/junctions before writing application state or launching code.
// A process already running as this user can still race these checks; OS account
// isolation, not this updater, is the boundary against another local principal.
fn require_plain_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = metadata.file_type().is_symlink();
                if linked {
                    return Err("Updater paths must not contain symbolic links or junctions".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot inspect updater path: {error}")),
        }
    }
    Ok(())
}

fn verify(
    root: &Root,
    bytes: &[u8],
    current: &str,
    os: &str,
    arch: &str,
    now: u64,
    receipt: Option<&Receipt>,
) -> Result<VerifiedUpdate, String> {
    if root.schema != 1 || bytes.len() as u64 > MAX_METADATA {
        return Err("Unsupported update metadata".into());
    }
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| "Malformed signed update envelope")?;
    let key = root
        .keys
        .iter()
        .find(|key| key.id == envelope.key_id && !root.revoked_key_ids.contains(&key.id))
        .ok_or("Update signing key is unknown or revoked")?;
    let key_bytes: [u8; 32] = STANDARD
        .decode(&key.public_key)
        .map_err(|_| "Invalid trusted update key")?
        .try_into()
        .map_err(|_| "Invalid trusted update key size")?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "Invalid trusted update key")?;
    let payload = STANDARD
        .decode(&envelope.payload)
        .map_err(|_| "Malformed update payload")?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(&envelope.signature)
            .map_err(|_| "Malformed update signature")?,
    )
    .map_err(|_| "Invalid update signature size")?;
    key.verify_strict(&signed_bytes(&payload), &signature)
        .map_err(|_| "Update signature verification failed")?;
    let manifest: Manifest =
        serde_json::from_slice(&payload).map_err(|_| "Malformed update manifest")?;
    if manifest.schema != 1
        || manifest.repository != REPOSITORY
        || manifest.channel != CHANNEL
        || manifest.os != os
        || manifest.arch != arch
    {
        return Err("Update does not match this repository, channel, OS, or architecture".into());
    }
    let version =
        semver::Version::parse(&manifest.version).map_err(|_| "Invalid update version")?;
    let current = semver::Version::parse(current).map_err(|_| "Invalid installed version")?;
    if version <= current || !version.pre.is_empty() || !version.build.is_empty() {
        return Err("Update would downgrade or repeat installed version".into());
    }
    if manifest.published_at > now
        || manifest.expires_at <= now
        || manifest.expires_at <= manifest.published_at
        || manifest.expires_at - manifest.published_at > MAX_LIFETIME
    {
        return Err("Update metadata is expired or outside its permitted validity period".into());
    }
    if manifest.sequence < root.minimum_sequence || manifest.sequence == 0 {
        return Err("Update sequence is below trusted minimum".into());
    }
    if manifest.size == 0
        || manifest.size > MAX_INSTALLER
        || manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("Invalid signed update size or digest".into());
    }
    let digest = format!("{:x}", Sha256::digest(&payload));
    if let Some(previous) = receipt
        && (manifest.sequence < previous.sequence
            || (manifest.sequence == previous.sequence && digest != previous.digest))
    {
        return Err("Update metadata replay or sequence collision rejected".into());
    }
    Ok(VerifiedUpdate {
        manifest,
        envelope: bytes.to_vec(),
        digest,
    })
}

fn secure_origin(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|p| p == 443)
}
fn allowed_download(url: &reqwest::Url) -> bool {
    secure_origin(url)
        && ((url.host_str() == Some("github.com")
            && url
                .path()
                .starts_with("/LaurentMaxhuni/kitlane/releases/download/"))
            || url.host_str() == Some("release-assets.githubusercontent.com"))
}
fn client() -> Result<Client, String> {
    Client::builder()
        .https_only(true)
        .user_agent(concat!("Kitlane/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || !allowed_download(attempt.url()) {
                attempt.error("Untrusted update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("Cannot initialize update transport: {e}"))
}
fn download(client: &Client, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|_| "Cannot reach official update release")?
        .error_for_status()
        .map_err(|e| {
            format!(
                "Official update request failed: {}",
                e.status().map(|s| s.as_u16()).unwrap_or(0)
            )
        })?;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err("Official update response exceeds size limit".into());
    }
    read_bounded(response, limit)
}

/// Checks only the official preview channel. This blocking API belongs on a worker thread.
pub fn check(state_dir: &Path) -> Result<CheckResult, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Ok(CheckResult::Unsupported("Automatic Kitlane installation is supported on Windows x64; use official releases on this platform.".into()));
    }
    let root = root()?;
    if root.keys.is_empty() {
        return Ok(CheckResult::NotConfigured(
            "Maintainer setup required: no trusted Kitlane release signing key is provisioned."
                .into(),
        ));
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        prerelease: bool,
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
    }
    let client = client()?;
    let mut releases: Vec<Release> = serde_json::from_slice(&download(
        &client,
        "https://api.github.com/repos/LaurentMaxhuni/kitlane/releases?per_page=10",
        1024 * 1024,
    )?)
    .map_err(|_| "Invalid official releases response")?;
    releases.sort_by(|a, b| {
        let version = |release: &Release| {
            release
                .tag_name
                .strip_prefix('v')
                .and_then(|v| semver::Version::parse(v).ok())
        };
        version(b).cmp(&version(a))
    });
    let receipt = read_receipt(state_dir)?;
    for release in releases
        .into_iter()
        .filter(|r| !r.draft && r.prerelease && r.assets.iter().any(|a| a.name == METADATA_NAME))
    {
        let Some(tag_version) = release.tag_name.strip_prefix('v') else {
            continue;
        };
        let Ok(version) = semver::Version::parse(tag_version) else {
            continue;
        };
        if version.to_string() != tag_version
            || !version.pre.is_empty()
            || !version.build.is_empty()
        {
            continue;
        }
        // A signed manifest for the installed release is not a pending update.
        if version
            <= semver::Version::parse(env!("CARGO_PKG_VERSION"))
                .map_err(|_| "Invalid current version")?
        {
            return Ok(CheckResult::Current);
        }
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/{}/{METADATA_NAME}",
            release.tag_name
        );
        let bytes = download(&client, &url, MAX_METADATA)?;
        let verified = verify(
            &root,
            &bytes,
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            now()?,
            receipt.as_ref(),
        )?;
        if verified.version() != tag_version {
            return Err("Signed version does not match official release tag".into());
        }
        record_accepted(state_dir, &verified)?;
        return Ok(CheckResult::Available(Box::new(verified)));
    }
    Ok(CheckResult::Current)
}

fn verify_artifact(mut reader: impl Read, manifest: &Manifest) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| format!("Cannot read staged installer: {e}"))?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > manifest.size {
            return Err("Installer exceeds signed size".into());
        }
        hash.update(&buffer[..count]);
    }
    if total != manifest.size || format!("{:x}", hash.finalize()) != manifest.sha256 {
        return Err("Installer size or SHA-256 does not match signed metadata".into());
    }
    Ok(())
}

/// Downloads to a fixed app-owned directory and rechecks exact signed bytes.
pub fn stage(update: &VerifiedUpdate, state_dir: &Path) -> Result<StagedUpdate, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Automatic installation is unsupported on this platform".into());
    }
    let receipt = read_receipt(state_dir)?;
    let update = verify(
        &root()?,
        &update.envelope,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        now()?,
        receipt.as_ref(),
    )?;
    fs::create_dir_all(state_dir).map_err(|e| format!("Cannot create updater directory: {e}"))?;
    let path = state_dir.join("Kitlane-verified-setup.exe");
    let partial = state_dir.join("Kitlane-download.partial");
    require_plain_path(&path)?;
    require_plain_path(&partial)?;
    let url = format!(
        "https://github.com/{REPOSITORY}/releases/download/v{0}/Kitlane-{0}-Setup-x64.exe",
        update.version()
    );
    let mut response = client()?
        .get(url)
        .send()
        .map_err(|_| "Cannot download official installer")?
        .error_for_status()
        .map_err(|_| "Official installer download failed")?;
    if response
        .content_length()
        .is_some_and(|n| n != update.manifest.size)
    {
        return Err("Installer response size disagrees with signed metadata".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(|e| {
            format!("Cannot create update staging file (another update may be running): {e}")
        })?;
    let result = (|| {
        let written = std::io::copy(
            &mut response.by_ref().take(update.manifest.size + 1),
            &mut file,
        )
        .map_err(|e| format!("Installer download interrupted: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Cannot flush staged installer: {e}"))?;
        if written != update.manifest.size {
            return Err("Installer download has incorrect size".into());
        }
        verify_artifact(
            File::open(&partial).map_err(|e| e.to_string())?,
            &update.manifest,
        )
    })();
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Cannot replace old staged installer: {e}"))?;
    }
    fs::rename(&partial, &path).map_err(|e| format!("Cannot finalize staged installer: {e}"))?;
    record_accepted(state_dir, &update)?;
    Ok(StagedUpdate {
        update,
        path,
        state_dir: state_dir.to_path_buf(),
    })
}

/// Starts a verified Inno installer. Caller must exit Kitlane so installer can replace it.
/// Fixed target and /NOCLOSEAPPLICATIONS avoid silently closing unrelated processes.
pub fn install(staged: &StagedUpdate) -> Result<u32, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Automatic installation is unsupported on this platform".into());
    }
    let receipt = read_receipt(&staged.state_dir)?;
    verify(
        &root()?,
        &staged.update.envelope,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        now()?,
        receipt.as_ref(),
    )?;
    require_plain_path(&staged.path)?;
    verify_artifact(
        File::open(&staged.path).map_err(|e| e.to_string())?,
        &staged.update.manifest,
    )?;
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?;
    let target = PathBuf::from(local).join("Programs").join("Kitlane");
    require_plain_path(&target)?;
    let own = std::env::current_exe().map_err(|e| e.to_string())?;
    let own_dir = own
        .parent()
        .ok_or("Executable has no installation directory")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if own_dir != target.canonicalize().map_err(|_| "Kitlane must be installed in its standard per-user directory before automatic updates")? || !target.join("unins000.exe").is_file() {
        return Err("Automatic updates require the standard per-user Kitlane installation".into());
    }
    let mut command = std::process::Command::new(&staged.path);
    command
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("KITLANE_TYPESAFE_API_KEY")
        .env_remove("KITLANE_UPDATE_SIGNING_KEY");
    command
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/NOCLOSEAPPLICATIONS",
            "/NORESTARTAPPLICATIONS",
            "/RESTARTMAINTENANCE=1",
        ])
        .arg(format!("/DIR={}", target.display()))
        .current_dir(&target);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map(|child| child.id())
        .map_err(|e| format!("Cannot start verified Kitlane installer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture() -> (Root, Vec<u8>, Manifest) {
        let signer = SigningKey::from_bytes(&[42; 32]);
        let root = Root {
            schema: 1,
            minimum_sequence: 1,
            keys: vec![RootKey {
                id: "fixture".into(),
                public_key: STANDARD.encode(signer.verifying_key().as_bytes()),
            }],
            revoked_key_ids: vec![],
        };
        let manifest = Manifest {
            schema: 1,
            repository: REPOSITORY.into(),
            channel: CHANNEL.into(),
            sequence: 2,
            version: "0.2.0".into(),
            published_at: 1_000,
            expires_at: 2_000,
            os: "windows".into(),
            arch: "x86_64".into(),
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        let payload = serde_json::to_vec(&manifest).unwrap();
        let envelope = Envelope {
            key_id: "fixture".into(),
            payload: STANDARD.encode(&payload),
            signature: STANDARD.encode(signer.sign(&signed_bytes(&payload)).to_bytes()),
        };
        (root, serde_json::to_vec(&envelope).unwrap(), manifest)
    }

    #[test]
    fn signed_fixture_accepts_expected_target() {
        let (root, envelope, _) = fixture();
        assert!(verify(&root, &envelope, "0.1.3", "windows", "x86_64", 1_500, None).is_ok());
    }

    #[test]
    fn rejects_bad_signature_unknown_and_revoked_keys() {
        let (mut root, mut bytes, _) = fixture();
        let original = bytes.clone();
        let mut envelope: Envelope = serde_json::from_slice(&bytes).unwrap();
        envelope.signature = STANDARD.encode([0; 64]);
        bytes = serde_json::to_vec(&envelope).unwrap();
        assert!(verify(&root, &bytes, "0.1.3", "windows", "x86_64", 1_500, None).is_err());
        root.revoked_key_ids.push("fixture".into());
        assert!(verify(&root, &original, "0.1.3", "windows", "x86_64", 1_500, None).is_err());
        root.keys.clear();
        assert!(verify(&root, &original, "0.1.3", "windows", "x86_64", 1_500, None).is_err());
    }

    #[test]
    fn signed_fields_and_payload_cannot_be_substituted() {
        let (root, original, manifest) = fixture();
        let sign_manifest = |manifest: &Manifest| {
            let signer = SigningKey::from_bytes(&[42; 32]);
            let payload = serde_json::to_vec(manifest).unwrap();
            serde_json::to_vec(&Envelope {
                key_id: "fixture".into(),
                payload: STANDARD.encode(&payload),
                signature: STANDARD.encode(signer.sign(&signed_bytes(&payload)).to_bytes()),
            })
            .unwrap()
        };
        for field in [
            "channel",
            "repository",
            "sha256",
            "size",
            "sequence",
            "lifetime",
        ] {
            let mut changed = manifest.clone();
            match field {
                "channel" => changed.channel = "stable".into(),
                "repository" => changed.repository = "evil/kitlane".into(),
                "sha256" => changed.sha256 = "bad".into(),
                "size" => changed.size = MAX_INSTALLER + 1,
                "sequence" => changed.sequence = 0,
                _ => changed.expires_at = changed.published_at + MAX_LIFETIME + 1,
            }
            assert!(
                verify(
                    &root,
                    &sign_manifest(&changed),
                    "0.1.3",
                    "windows",
                    "x86_64",
                    1_500,
                    None
                )
                .is_err()
            );
        }
        let mut envelope: Envelope = serde_json::from_slice(&original).unwrap();
        let mut changed = manifest;
        changed.version = "0.9.0".into();
        envelope.payload = STANDARD.encode(serde_json::to_vec(&changed).unwrap());
        assert!(
            verify(
                &root,
                &serde_json::to_vec(&envelope).unwrap(),
                "0.1.3",
                "windows",
                "x86_64",
                1_500,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn history_corruption_and_ledger_replay_fail_closed() {
        let temp = std::env::temp_dir().join(format!(
            "kitlane-update-fixture-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        let temp = fs::canonicalize(temp).unwrap();
        let (root, bytes, _) = fixture();
        let verified = verify(&root, &bytes, "0.1.3", "windows", "x86_64", 1_500, None).unwrap();
        record_accepted(&temp, &verified).unwrap();
        assert_eq!(read_receipt(&temp).unwrap().unwrap().sequence, 2);
        fs::write(temp.join("accepted-update.json"), b"corrupt").unwrap();
        assert!(read_receipt(&temp).is_err());
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_expiry_future_wrong_target_and_downgrade() {
        let (root, bytes, _) = fixture();
        for (version, os, arch, now) in [
            ("0.2.0", "windows", "x86_64", 1_500),
            ("0.3.0", "windows", "x86_64", 1_500),
            ("0.1.3", "linux", "x86_64", 1_500),
            ("0.1.3", "windows", "aarch64", 1_500),
            ("0.1.3", "windows", "x86_64", 2_000),
            ("0.1.3", "windows", "x86_64", 500),
        ] {
            assert!(verify(&root, &bytes, version, os, arch, now, None).is_err());
        }
    }

    #[test]
    fn replay_floor_allows_only_identical_retry() {
        let (root, bytes, _) = fixture();
        let accepted = verify(&root, &bytes, "0.1.3", "windows", "x86_64", 1_500, None).unwrap();
        let mut receipt = Receipt {
            sequence: 2,
            digest: accepted.digest.clone(),
        };
        assert!(
            verify(
                &root,
                &bytes,
                "0.1.3",
                "windows",
                "x86_64",
                1_500,
                Some(&receipt)
            )
            .is_ok()
        );
        receipt.digest = "tampered".into();
        assert!(
            verify(
                &root,
                &bytes,
                "0.1.3",
                "windows",
                "x86_64",
                1_500,
                Some(&receipt)
            )
            .is_err()
        );
        receipt.sequence = 3;
        assert!(
            verify(
                &root,
                &bytes,
                "0.1.3",
                "windows",
                "x86_64",
                1_500,
                Some(&receipt)
            )
            .is_err()
        );
    }

    #[test]
    fn downloads_and_hashes_are_bounded() {
        assert_eq!(read_bounded(&b"abc"[..], 3).unwrap(), b"abc");
        assert!(read_bounded(&b"abcd"[..], 3).is_err());
        let (_, _, manifest) = fixture();
        assert!(verify_artifact(&b"abc"[..], &manifest).is_ok());
        assert!(verify_artifact(&b"abd"[..], &manifest).is_err());
        assert!(verify_artifact(&b"abcd"[..], &manifest).is_err());
        assert!(verify_artifact(&b"ab"[..], &manifest).is_err());
    }

    #[test]
    fn official_urls_only() {
        assert!(allowed_download(&reqwest::Url::parse("https://github.com/LaurentMaxhuni/kitlane/releases/download/v0.2.0/Kitlane-0.2.0-Setup-x64.exe").unwrap()));
        for url in [
            "http://github.com/LaurentMaxhuni/kitlane/releases/download/x",
            "https://github.com/evil/kitlane/releases/download/x",
            "https://github.com.evil.test/LaurentMaxhuni/kitlane/releases/download/x",
            "https://user@github.com/LaurentMaxhuni/kitlane/releases/download/x",
            "https://release-assets.githubusercontent.com.evil.test/x",
        ] {
            assert!(!allowed_download(&reqwest::Url::parse(url).unwrap()));
        }
    }
}
