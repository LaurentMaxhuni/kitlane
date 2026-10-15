//! Explicit, blocking TypeSafe calls. Invoke from the worker after provider consent.
use std::{fmt, io::Read, sync::Arc, time::Duration};

use kitlane_core::{
    model::{InstalledApp, Platform},
    suggestions::{Evaluation, Preferences, Routing, Suggestion, SuggestionError},
};
use kitlane_sync_client::{OsVault, Vault};
use reqwest::{
    blocking::Client as HttpClient,
    header::{self, HeaderValue},
    redirect::Policy,
};
use zeroize::Zeroizing;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const ACCOUNT: &str = "kitlane.ai.typesafe.v1";
const MAX_RESPONSE: u64 = 128 * 1024;

/// Fixed messages only. Transport/server/vault error text must not expose secrets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    MissingKey,
    InvalidKey,
    VaultUnavailable,
    Unavailable,
    Unauthorized,
    RateLimited,
    Rejected,
    InvalidResponse,
    TooLarge,
    Suggestions(SuggestionError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Suggestions(error) => error.fmt(f),
            _ => f.write_str(match self {
                Self::MissingKey => "Configure a TypeSafe API key before requesting alternatives.",
                Self::InvalidKey => {
                    "API key must contain 8–512 printable ASCII characters without spaces."
                }
                Self::VaultUnavailable => {
                    "OS credential storage is unavailable. Unlock it and try again."
                }
                Self::Unavailable => "TypeSafe is unavailable or timed out. Try again explicitly.",
                Self::Unauthorized => "TypeSafe rejected the API key. Check its access and expiry.",
                Self::RateLimited => "TypeSafe rate limit reached. Wait before requesting again.",
                Self::Rejected => {
                    "TypeSafe rejected this request. Check provider access and account status."
                }
                Self::InvalidResponse => {
                    "TypeSafe returned an invalid response. Try again explicitly."
                }
                Self::TooLarge => "TypeSafe response exceeded the permitted size.",
                Self::Suggestions(_) => unreachable!(),
            }),
        }
    }
}
impl std::error::Error for Error {}
impl From<SuggestionError> for Error {
    fn from(error: SuggestionError) -> Self {
        Self::Suggestions(error)
    }
}

pub struct Client {
    http: HttpClient,
    vault: Arc<dyn Vault>,
}
impl Client {
    pub fn new() -> Result<Self, Error> {
        Self::with_vault(Arc::new(OsVault))
    }

    fn with_vault(vault: Arc<dyn Vault>) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .https_only(true)
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .user_agent("Kitlane/0.1 AI-alternatives")
            .build()
            .map_err(|_| Error::Unavailable)?;
        Ok(Self { http, vault })
    }

    pub fn configured(&self) -> Result<bool, Error> {
        self.key().map(|key| key.is_some())
    }

    pub fn save_key(&self, key: Zeroizing<String>) -> Result<(), Error> {
        validate_key(&key)?;
        self.vault
            .set(ACCOUNT, &key)
            .map_err(|_| Error::VaultUnavailable)
    }

    /// Removes only the saved key. A process environment key remains available.
    pub fn clear_key(&self) -> Result<(), Error> {
        self.vault
            .delete(ACCOUNT)
            .map_err(|_| Error::VaultUnavailable)
    }

    fn key(&self) -> Result<Option<Zeroizing<String>>, Error> {
        let saved = self
            .vault
            .get(ACCOUNT)
            .map_err(|_| Error::VaultUnavailable)?
            .map(Zeroizing::new);
        let key = saved.or_else(environment_key);
        if let Some(key) = &key {
            validate_key(key)?;
        }
        Ok(key)
    }

    /// Sends only the selected app name/publisher, preferences and reviewed candidates.
    /// No retries: a network failure requires a new explicit user request.
    pub fn suggest(
        &self,
        app: &InstalledApp,
        preferences: &Preferences,
        target: Platform,
    ) -> Result<Vec<Suggestion>, Error> {
        let key = self.key()?.ok_or(Error::MissingKey)?;
        let evaluation = match Evaluation::prepare(app, preferences, target) {
            Ok(evaluation) => evaluation,
            Err(SuggestionError::NoCandidates) => {
                let routing = Routing::prepare(app, preferences)?;
                let bytes = self.request(routing.body(), &key)?;
                let Some(category) = routing.finish(&bytes)? else {
                    return Ok(Vec::new());
                };
                Evaluation::prepare_category(app, preferences, target, &category)?
            }
            Err(error) => return Err(error.into()),
        };
        self.evaluate(evaluation, &key)
    }

    fn evaluate(&self, evaluation: Evaluation, key: &str) -> Result<Vec<Suggestion>, Error> {
        let bytes = self.request(evaluation.body(), key)?;
        evaluation.finish(&bytes).map_err(Error::from)
    }

    fn request(&self, body: &serde_json::Value, key: &str) -> Result<Vec<u8>, Error> {
        validate_key(key)?;
        let auth = Zeroizing::new(format!("Bearer {key}"));
        let mut authorization = HeaderValue::from_str(&auth).map_err(|_| Error::InvalidKey)?;
        authorization.set_sensitive(true);
        let body = serde_json::to_vec(body).map_err(|_| Error::InvalidResponse)?;
        let response = self
            .http
            .post(ENDPOINT)
            .header(header::AUTHORIZATION, authorization)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .map_err(|_| Error::Unavailable)?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(Error::Unauthorized),
            429 => return Err(Error::RateLimited),
            500..=599 => return Err(Error::Unavailable),
            _ => return Err(Error::Rejected),
        }
        if response.content_length().is_some_and(|n| n > MAX_RESPONSE) {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.len() as u64 > MAX_RESPONSE {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
}

fn environment_key() -> Option<Zeroizing<String>> {
    std::env::var("TYPESAFE_API_KEY")
        .ok()
        .map(Zeroizing::new)
        .filter(|key| !key.is_empty())
}

fn validate_key(key: &str) -> Result<(), Error> {
    if !(8..=512).contains(&key.len()) || !key.bytes().all(|b| b.is_ascii_graphic()) {
        Err(Error::InvalidKey)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitlane_core::model::{InstallScope, InventorySource};
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryVault(Mutex<Option<Zeroizing<String>>>);
    impl Vault for MemoryVault {
        fn get(&self, account: &str) -> Result<Option<String>, kitlane_sync_client::Error> {
            assert_eq!(account, ACCOUNT);
            Ok(self.0.lock().unwrap().as_ref().map(|s| s.to_string()))
        }
        fn set(&self, account: &str, value: &str) -> Result<(), kitlane_sync_client::Error> {
            assert_eq!(account, ACCOUNT);
            *self.0.lock().unwrap() = Some(Zeroizing::new(value.to_owned()));
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), kitlane_sync_client::Error> {
            assert_eq!(account, ACCOUNT);
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }
    #[test]
    fn key_validation_and_os_vault_lifecycle() {
        let vault = Arc::new(MemoryVault::default());
        let client = Client::with_vault(vault.clone()).unwrap();
        for invalid in [
            "",
            "short",
            "with spaces",
            "header\r\ninjection",
            "nonasciié",
        ] {
            assert_eq!(
                client.save_key(Zeroizing::new(invalid.into())),
                Err(Error::InvalidKey)
            );
        }
        client
            .save_key(Zeroizing::new("test-secret-value".into()))
            .unwrap();
        assert!(client.configured().unwrap());
        assert_eq!(
            client.key().unwrap().as_deref().map(String::as_str),
            Some("test-secret-value")
        );
        client.clear_key().unwrap();
        assert!(vault.0.lock().unwrap().is_none());
    }

    /// Explicit opt-in only. Four synthetic app cases; never reads the local inventory.
    #[test]
    #[ignore = "requires explicit network consent and TYPESAFE_API_KEY; incurs provider usage"]
    fn live_representative_alternatives() {
        let key = environment_key().expect("TYPESAFE_API_KEY must be configured");
        let client = Client::with_vault(Arc::new(MemoryVault::default())).unwrap();
        let cases = [
            (
                "Google Chrome",
                "Google LLC",
                Platform::Windows,
                "Block trackers using built-in browser features",
                true,
                Some("brave"),
            ),
            (
                "Autodesk AutoCAD",
                "Autodesk",
                Platform::Windows,
                "Open source parametric engineering CAD for product design",
                true,
                Some("freecad"),
            ),
            (
                "Microsoft Office",
                "Microsoft Corporation",
                Platform::Windows,
                "Open source offline word processing and spreadsheets",
                true,
                Some("libreoffice"),
            ),
            (
                "Google Chrome",
                "Google LLC",
                Platform::Windows,
                "Must edit multi-track audio offline; web browsers cannot meet this requirement",
                false,
                None,
            ),
        ];
        for (name, publisher, target, priorities, open_source_only, expected) in cases {
            let app = InstalledApp {
                id: "synthetic-case".into(),
                name: name.into(),
                publisher: Some(publisher.into()),
                version: None,
                scope: InstallScope::User,
                source: InventorySource::WindowsRegistry,
                native_id: None,
                origin: None,
            };
            let preferences = Preferences {
                free_only: true,
                open_source_only,
                priorities: priorities.into(),
            };
            let evaluation = match Evaluation::prepare(&app, &preferences, target) {
                Ok(e) => e,
                Err(SuggestionError::NoCandidates) => {
                    let routing = Routing::prepare(&app, &preferences).unwrap();
                    let bytes = client.request(routing.body(), &key).unwrap();
                    let category = routing
                        .finish(&bytes)
                        .unwrap()
                        .expect("representative source should route");
                    Evaluation::prepare_category(&app, &preferences, target, &category).unwrap()
                }
                Err(error) => panic!("{error}"),
            };
            let result = client.evaluate(evaluation, &key).unwrap();
            eprintln!(
                "{name} / {}: {}",
                target.label(),
                result
                    .iter()
                    .map(|s| format!("{}={:.3}/{:.3}", s.product_id, s.score, s.confidence))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            match expected {
                Some(id) => assert!(
                    result.iter().any(|s| s.product_id == id),
                    "expected reviewed alternative missing"
                ),
                None => assert!(result.is_empty(), "incompatible request should abstain"),
            }
        }
    }
}
