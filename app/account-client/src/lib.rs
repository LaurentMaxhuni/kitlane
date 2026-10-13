mod handoff;

use kitlane_core::model::{DeviceInfo, InventorySnapshot};
use kitlane_sync_client::{OsVault, Vault};
use reqwest::{
    Method, StatusCode, Url,
    blocking::{Client as Http, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Network,
    Unauthorized,
    Vault,
    InvalidResponse,
    InvalidCode,
    RateLimited,
    Cancelled,
    DeviceOwnership,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self{
        Self::Configuration=>"Account service is not configured. Set the HTTPS API address before building Kitlane.",
        Self::Network=>"Cannot reach the account service. Check your connection and try again.",
        Self::Unauthorized=>"Your session expired or was revoked. Sign in with Google again.",
        Self::Vault=>"Cannot access the OS credential store. Unlock it and try signing in again.",
        Self::InvalidResponse=>"The account service returned an invalid response.",
        Self::InvalidCode=>"Code unavailable. Generate a new code on a device signed in to the same Google account.",
        Self::RateLimited=>"Too many attempts. Try again later.",
        Self::Cancelled=>"Sign-in cancelled or timed out. Try again.",
        Self::DeviceOwnership=>"Device ownership could not be verified. Restore this installation's OS credentials before signing in.",
    })
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub account_id: String,
    pub email: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairCode {
    pub code: String,
    pub expires_at: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    access_token: String,
    refresh_token: String,
    account: Account,
    device_id: String,
    expires_at: u64,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

pub struct Client {
    http: Http,
    endpoint: String,
    device: DeviceInfo,
    vault: Arc<dyn Vault>,
    key: String,
    credentials: Option<Credentials>,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
pub fn valid_code(value: &str) -> bool {
    value.len() == 8
        && value
            .bytes()
            .all(|b| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&b))
}
fn opaque(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Client {
    pub fn new(endpoint: &str, device: &DeviceInfo) -> Result<Self, Error> {
        Self::with_vault(endpoint, device, Arc::new(OsVault))
    }
    pub fn with_vault(
        endpoint: &str,
        device: &DeviceInfo,
        vault: Arc<dyn Vault>,
    ) -> Result<Self, Error> {
        let url = Url::parse(endpoint).map_err(|_| Error::Configuration)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Configuration);
        }
        device.validate().map_err(|_| Error::Configuration)?;
        let endpoint = url.origin().ascii_serialization();
        let key = format!(
            "cloud-v2-{:x}-{}",
            Sha256::digest(endpoint.as_bytes()),
            device.id
        );
        let http = Http::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Error::Network)?;
        Ok(Self {
            http,
            endpoint,
            device: device.clone(),
            vault,
            key,
            credentials: None,
        })
    }
    fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
        authenticated: bool,
    ) -> Result<Response, Error> {
        let mut req = self
            .http
            .request(method, format!("{}{}", self.endpoint, path));
        if authenticated {
            req = req.bearer_auth(
                &self
                    .credentials
                    .as_ref()
                    .ok_or(Error::Unauthorized)?
                    .access_token,
            );
        }
        if let Some(body) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(Zeroizing::new(body.to_string()).to_string());
        }
        let response = req.send().map_err(|_| Error::Network)?;
        match response.status() {
            StatusCode::OK => Ok(response),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Error::Unauthorized),
            StatusCode::BAD_REQUEST => Err(Error::InvalidCode),
            StatusCode::TOO_MANY_REQUESTS => Err(Error::RateLimited),
            StatusCode::CONFLICT => Err(Error::DeviceOwnership),
            s if s.is_server_error() => Err(Error::Network),
            _ => Err(Error::InvalidResponse),
        }
    }
    fn decode<T: DeserializeOwned>(mut response: Response, limit: usize) -> Result<T, Error> {
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(Error::InvalidResponse);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        response
            .by_ref()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Network)?;
        if bytes.len() > limit {
            return Err(Error::InvalidResponse);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
    }
    fn accept(&mut self, c: Credentials) -> Result<Account, Error> {
        let check = DeviceInfo {
            id: c.account.account_id.clone(),
            name: c.account.name.clone(),
            platform: self.device.platform,
            architecture: self.device.architecture.clone(),
        };
        if c.device_id != self.device.id
            || !opaque(&c.access_token)
            || !opaque(&c.refresh_token)
            || check.validate().is_err()
            || c.account.email.is_empty()
            || c.account.email.len() > 320
            || c.account.email.chars().any(char::is_control)
            || c.expires_at <= now()
            || c.expires_at > now() + 1200
        {
            return Err(Error::InvalidResponse);
        }
        let encoded =
            Zeroizing::new(serde_json::to_string(&c).map_err(|_| Error::InvalidResponse)?);
        if self.vault.set(&self.key, &encoded).is_err() {
            self.credentials = None;
            return Err(Error::Vault);
        }
        let account = c.account.clone();
        self.credentials = Some(c);
        Ok(account)
    }
    pub fn restore(&mut self) -> Result<Option<Account>, Error> {
        let Some(saved) = self
            .vault
            .get(&self.key)
            .map_err(|_| Error::Vault)?
            .map(Zeroizing::new)
        else {
            return Ok(None);
        };
        if saved.len() > 8192 {
            return Err(Error::Vault);
        }
        let credentials: Credentials = serde_json::from_str(&saved).map_err(|_| Error::Vault)?;
        let response=self.request(Method::POST,"/v2/auth/refresh",Some(serde_json::json!({"refresh_token":credentials.refresh_token,"device_id":self.device.id})),false);
        match response {
            Ok(r) => self.accept(Self::decode(r, 8192)?).map(Some),
            Err(Error::Unauthorized) => {
                self.credentials = None;
                self.vault.delete(&self.key).map_err(|_| Error::Vault)?;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    fn ensure(&mut self) -> Result<(), Error> {
        if self
            .credentials
            .as_ref()
            .is_none_or(|c| c.expires_at <= now() + 30)
        {
            match self.restore() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    self.credentials = None;
                    return Err(Error::Unauthorized);
                }
                Err(error) => {
                    self.credentials = None;
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub fn sign_in(&mut self, cancel: &std::sync::atomic::AtomicBool) -> Result<Account, Error> {
        let result = handoff::sign_in(self, cancel)?;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        self.accept(result)
    }
    pub fn update_device(&mut self, device: &DeviceInfo) -> Result<(), Error> {
        if device.id != self.device.id || device.validate().is_err() {
            return Err(Error::InvalidResponse);
        }
        self.device = device.clone();
        if self.credentials.is_none() {
            return Ok(());
        }
        self.ensure()?;
        self.request(
            Method::PUT,
            "/v2/device",
            Some(serde_json::to_value(device).map_err(|_| Error::InvalidResponse)?),
            true,
        )?;
        Ok(())
    }
    pub fn maintain_session(&mut self) -> Result<(), Error> {
        self.ensure()
    }
    pub fn session_valid(&self) -> bool {
        self.credentials
            .as_ref()
            .is_some_and(|c| c.expires_at > now())
    }
    fn installation_proof(&self) -> Result<Zeroizing<String>, Error> {
        let key = format!("{}-installation", self.key);
        if let Some(value) = self
            .vault
            .get(&key)
            .map_err(|_| Error::Vault)?
            .map(Zeroizing::new)
        {
            return if opaque(&value) {
                Ok(value)
            } else {
                Err(Error::Vault)
            };
        }
        let value = handoff::random()?;
        self.vault.set(&key, &value).map_err(|_| Error::Vault)?;
        Ok(value)
    }
    pub fn sign_out(&mut self) -> Result<(), Error> {
        let remote = if let Some(credentials) = &self.credentials {
            self.request(Method::POST, "/v2/auth/revoke", Some(serde_json::json!({"refresh_token":credentials.refresh_token,"device_id":self.device.id})), false)
                .map(|_| ())
        } else {
            Ok(())
        };
        self.credentials = None;
        self.vault.delete(&self.key).map_err(|_| Error::Vault)?;
        remote
    }
    pub fn create_pairing_code(&mut self) -> Result<PairCode, Error> {
        self.ensure()?;
        let code: PairCode = Self::decode(
            self.request(Method::POST, "/v2/pairing/codes", None, true)?,
            8192,
        )?;
        if !valid_code(&code.code) || code.expires_at <= now() || code.expires_at > now() + 360 {
            return Err(Error::InvalidResponse);
        }
        Ok(code)
    }
    pub fn pair(&mut self, code: &str) -> Result<Vec<DeviceInfo>, Error> {
        if !valid_code(code) {
            return Err(Error::InvalidCode);
        }
        self.ensure()?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Devices {
            devices: Vec<DeviceInfo>,
        }
        let result: Devices = Self::decode(
            self.request(
                Method::POST,
                "/v2/pairing/consume",
                Some(serde_json::json!({"code":code})),
                true,
            )?,
            65536,
        )?;
        self.validate_devices(result.devices)
    }
    fn validate_devices(&self, devices: Vec<DeviceInfo>) -> Result<Vec<DeviceInfo>, Error> {
        if devices.len() > 128
            || devices
                .iter()
                .any(|d| d.id == self.device.id || d.validate().is_err())
        {
            return Err(Error::InvalidResponse);
        }
        let mut ids = std::collections::HashSet::new();
        if devices.iter().any(|d| !ids.insert(&d.id)) {
            return Err(Error::InvalidResponse);
        }
        Ok(devices)
    }
    pub fn devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        self.ensure()?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Devices {
            devices: Vec<DeviceInfo>,
        }
        let result: Devices =
            Self::decode(self.request(Method::GET, "/v2/devices", None, true)?, 65536)?;
        self.validate_devices(result.devices)
    }
    pub fn sync(&mut self, snapshot: &InventorySnapshot) -> Result<Vec<InventorySnapshot>, Error> {
        self.ensure()?;
        if snapshot.device.id != self.device.id || snapshot.validate().is_err() {
            return Err(Error::InvalidResponse);
        }
        let value = serde_json::to_value(snapshot).map_err(|_| Error::InvalidResponse)?;
        if value.to_string().len() > 4 * 1024 * 1024 {
            return Err(Error::InvalidResponse);
        }
        self.request(Method::PUT, "/v2/inventory", Some(value), true)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Peers {
            peers: Vec<InventorySnapshot>,
        }
        let peers: Peers = Self::decode(
            self.request(Method::GET, "/v2/inventory", None, true)?,
            16 * 1024 * 1024,
        )?;
        if peers.peers.len() > 128
            || peers
                .peers
                .iter()
                .any(|p| p.device.id == self.device.id || p.validate().is_err())
        {
            return Err(Error::InvalidResponse);
        }
        Ok(peers.peers)
    }
}

#[cfg(test)]
mod tests;
