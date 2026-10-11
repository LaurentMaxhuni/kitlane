//! Explicit, blocking inventory sync for the loopback-only prototype.
//! Run on a dedicated worker, never an async runtime or the UI thread.
use kitlane_core::model::{DeviceInfo, InventorySnapshot};
use reqwest::{Method, StatusCode, blocking::Response, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fmt,
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8787";
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_DEVICES: usize = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Fixed messages only: never retain transport, server, or credential-store errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidEndpoint,
    InvalidDevice,
    InvalidPairingToken,
    InvalidInventory,
    NotEnrolled,
    AlreadyEnrolled,
    VaultUnavailable,
    EnrollmentVaultFailed,
    EnrollmentUncertain,
    Unauthorized,
    Forbidden,
    Conflict,
    Unavailable,
    InvalidResponse,
    TooLarge,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidEndpoint => "Use http://127.0.0.1:<port> without a path or credentials.",
            Self::InvalidDevice => "Device identity or metadata is invalid.",
            Self::InvalidPairingToken => "Pairing token must contain 64 hexadecimal characters.",
            Self::InvalidInventory => "Local inventory is invalid or does not match this device.",
            Self::NotEnrolled => "Enroll this device before syncing.",
            Self::AlreadyEnrolled => "A credential already exists. Forget it explicitly before enrolling again.",
            Self::VaultUnavailable => "OS credential store is unavailable. Unlock or restore it and try again.",
            Self::EnrollmentVaultFailed => "Enrollment succeeded, but OS credential storage failed. The grant was consumed. Ask the owner for a recovery pairing grant for this device, then enroll explicitly.",
            Self::EnrollmentUncertain => "Enrollment could not be confirmed. Do not retry this grant automatically. Ask the owner to check the enrollment and issue a recovery pairing grant for this device if needed.",
            Self::Unauthorized => "Credential or pairing grant was rejected. Ask the owner to check revocation or expiry; enrollment requires user action.",
            Self::Forbidden => "Service rejected this device's permissions.",
            Self::Conflict => "Server revision or identity changed. Check device metadata and sync again explicitly.",
            Self::Unavailable => "Sync service is unavailable or timed out. Try again explicitly; current server state will be checked first.",
            Self::InvalidResponse => "Service returned invalid inventory or device metadata.",
            Self::TooLarge => "Inventory exceeds the bounded sync data limit.",
        })
    }
}
impl std::error::Error for Error {}

/// Injectable for tests. Production uses the OS credential store exclusively.
/// Implementations must never log secrets or persist them in plaintext files.
pub trait Vault: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, Error>;
    fn set(&self, account: &str, token: &str) -> Result<(), Error>;
    fn delete(&self, account: &str) -> Result<(), Error>;
}
struct OsVault;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl OsVault {
    fn entry(account: &str) -> Result<keyring::Entry, Error> {
        keyring::Entry::new("kitlane.sync.loopback.v1", account)
            .map_err(|_| Error::VaultUnavailable)
    }
}
impl Vault for OsVault {
    fn get(&self, account: &str) -> Result<Option<String>, Error> {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        return match Self::entry(account)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::VaultUnavailable),
        };
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = account;
            Err(Error::VaultUnavailable)
        }
    }
    fn set(&self, account: &str, token: &str) -> Result<(), Error> {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        return Self::entry(account)?
            .set_password(token)
            .map_err(|_| Error::VaultUnavailable);
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = (account, token);
            Err(Error::VaultUnavailable)
        }
    }
    fn delete(&self, account: &str) -> Result<(), Error> {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        return match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(Error::VaultUnavailable),
        };
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            let _ = account;
            Err(Error::VaultUnavailable)
        }
    }
}

pub struct Client {
    http: reqwest::blocking::Client,
    endpoint: String,
    device: DeviceInfo,
    account: String,
    vault: Arc<dyn Vault>,
    operation: Mutex<()>,
}
#[derive(Debug)]
pub struct SyncResult {
    pub peers: Vec<InventorySnapshot>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    device_token: String,
    device: DeviceInfo,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Summary {
    device: DeviceInfo,
    revision: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Update {
    revision: u64,
    snapshot: InventorySnapshot,
}

fn endpoint(value: &str) -> Result<String, Error> {
    let port = value
        .strip_prefix("http://127.0.0.1:")
        .ok_or(Error::InvalidEndpoint)?;
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidEndpoint);
    }
    let port: u16 = port.parse().map_err(|_| Error::InvalidEndpoint)?;
    if port == 0 {
        return Err(Error::InvalidEndpoint);
    }
    Ok(format!("http://127.0.0.1:{port}"))
}
fn valid_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Client {
    pub fn new(endpoint: &str, device: &DeviceInfo) -> Result<Self, Error> {
        Self::with_vault(endpoint, device, Arc::new(OsVault))
    }
    pub fn with_vault(
        value: &str,
        device: &DeviceInfo,
        vault: Arc<dyn Vault>,
    ) -> Result<Self, Error> {
        let endpoint = endpoint(value)?;
        device.validate().map_err(|_| Error::InvalidDevice)?;
        let account = format!(
            "{:x}",
            Sha256::digest(format!("{endpoint}\n{}", device.id).as_bytes())
        );
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .http1_only()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(Duration::from_secs(2))
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| Error::Unavailable)?;
        Ok(Self {
            http,
            endpoint,
            device: device.clone(),
            account,
            vault,
            operation: Mutex::new(()),
        })
    }
    fn token(&self) -> Result<Option<Zeroizing<String>>, Error> {
        let token = self
            .vault
            .get(&self.account)
            .map_err(|_| Error::VaultUnavailable)?
            .map(Zeroizing::new);
        if token.as_ref().is_some_and(|s| !valid_token(s)) {
            return Err(Error::VaultUnavailable);
        }
        Ok(token)
    }
    /// Local vault presence only; no network request or server revocation check.
    pub fn is_enrolled(&self) -> Result<bool, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::VaultUnavailable)?;
        Ok(self.token()?.is_some())
    }
    /// Forget local credentials only. Server enrollment is not revoked.
    pub fn disconnect(&self) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::VaultUnavailable)?;
        self.vault
            .delete(&self.account)
            .map_err(|_| Error::VaultUnavailable)
    }
    /// Single-use grant exchange. Never automatically retries or replaces a credential.
    pub fn enroll(&self, pairing_token: &str) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::VaultUnavailable)?;
        if self.token()?.is_some() {
            return Err(Error::AlreadyEnrolled);
        }
        if !valid_token(pairing_token) {
            return Err(Error::InvalidPairingToken);
        }
        let body = Zeroizing::new(
            serde_json::to_vec(&serde_json::json!({"pairing_token":pairing_token}))
                .map_err(|_| Error::InvalidPairingToken)?,
        );
        let response = self
            .request(
                Method::POST,
                "/v1/enroll",
                None,
                Some(&body),
                Instant::now() + REQUEST_TIMEOUT,
            )
            .map_err(|error| {
                if error == Error::Unavailable {
                    Error::EnrollmentUncertain
                } else {
                    error
                }
            })?;
        if response.status() != StatusCode::CREATED {
            return Err(if response.status().is_server_error() {
                Error::EnrollmentUncertain
            } else {
                status_error(response.status())
            });
        }
        let mut remaining = MAX_BODY;
        let enrollment: Enrollment =
            decode(response, &mut remaining).map_err(|_| Error::EnrollmentUncertain)?;
        let token = Zeroizing::new(enrollment.device_token);
        if enrollment.device != self.device
            || enrollment.device.validate().is_err()
            || !valid_token(&token)
        {
            return Err(Error::EnrollmentUncertain);
        }
        self.vault
            .set(&self.account, &token)
            .map_err(|_| Error::EnrollmentVaultFailed)
    }
    /// Upload only when explicitly called. Always read current revision and snapshot first.
    pub fn sync(&self, local: &InventorySnapshot) -> Result<SyncResult, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::VaultUnavailable)?;
        local.validate().map_err(|_| Error::InvalidInventory)?;
        if local.device != self.device {
            return Err(Error::InvalidInventory);
        }
        let token = self.token()?.ok_or(Error::NotEnrolled)?;
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        // Aggregate inbound JSON is bounded, as well as each individual response.
        let mut remaining = MAX_BODY;
        let response = self.request(Method::GET, "/v1/devices", Some(&token), None, deadline)?;
        let roster: Vec<Summary> = decode_ok(response, &mut remaining)?;
        if roster.len() > MAX_DEVICES {
            return Err(Error::TooLarge);
        }
        let mut ids = HashSet::new();
        for item in &roster {
            if item.device.validate().is_err() || !ids.insert(&item.device.id) {
                return Err(Error::InvalidResponse);
            }
        }
        let own = roster
            .iter()
            .find(|s| s.device.id == self.device.id)
            .ok_or(Error::Unauthorized)?;
        if own.device != self.device {
            return Err(Error::InvalidResponse);
        }
        let mut current = None;
        let mut peers = Vec::new();
        for item in &roster {
            if item.revision == 0 {
                continue;
            }
            let response = self.request(
                Method::GET,
                &format!("/v1/snapshots/{}", item.device.id),
                Some(&token),
                None,
                deadline,
            )?;
            let update: Update = decode_ok(response, &mut remaining)?;
            if update.revision != item.revision || update.revision == 0 {
                return Err(Error::Conflict);
            }
            if update.snapshot.device != item.device || update.snapshot.validate().is_err() {
                return Err(Error::InvalidResponse);
            }
            if item.device.id == self.device.id {
                current = Some(update.snapshot);
            } else {
                peers.push(update.snapshot);
            }
        }
        // A previous timed-out PUT may already have committed. Do not replay it.
        if current.as_ref() != Some(local) {
            let revision = own.revision.checked_add(1).ok_or(Error::Conflict)?;
            let body = serde_json::to_vec(&Update {
                revision,
                snapshot: local.clone(),
            })
            .map_err(|_| Error::InvalidInventory)?;
            if body.len() > MAX_BODY {
                return Err(Error::TooLarge);
            }
            let response = self.request(
                Method::PUT,
                &format!("/v1/snapshots/{}", self.device.id),
                Some(&token),
                Some(&body),
                deadline,
            )?;
            if response.status() != StatusCode::NO_CONTENT {
                return Err(status_error(response.status()));
            }
        }
        Ok(SyncResult { peers })
    }
    fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&[u8]>,
        deadline: Instant,
    ) -> Result<Response, Error> {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or(Error::Unavailable)?
            .min(REQUEST_TIMEOUT);
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.endpoint))
            .timeout(timeout);
        if let Some(token) = token {
            let mut value = header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| Error::VaultUnavailable)?;
            value.set_sensitive(true);
            request = request.header(header::AUTHORIZATION, value);
        }
        if let Some(body) = body {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.to_vec());
        }
        request.send().map_err(|_| Error::Unavailable)
    }
}
fn status_error(status: StatusCode) -> Error {
    match status {
        StatusCode::UNAUTHORIZED => Error::Unauthorized,
        StatusCode::FORBIDDEN => Error::Forbidden,
        StatusCode::CONFLICT => Error::Conflict,
        StatusCode::PAYLOAD_TOO_LARGE => Error::TooLarge,
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::INTERNAL_SERVER_ERROR => Error::Unavailable,
        _ => Error::InvalidResponse,
    }
}
fn decode_ok<T: DeserializeOwned>(response: Response, remaining: &mut usize) -> Result<T, Error> {
    if response.status() != StatusCode::OK {
        return Err(status_error(response.status()));
    }
    decode(response, remaining)
}
fn decode<T: DeserializeOwned>(response: Response, remaining: &mut usize) -> Result<T, Error> {
    let limit = (*remaining).min(MAX_BODY);
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(Error::TooLarge);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Unavailable)?;
    if bytes.len() > limit {
        return Err(Error::TooLarge);
    }
    *remaining -= bytes.len();
    serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
}
