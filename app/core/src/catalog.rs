//! Reviewed metadata associations, never binary identity or installation authorization.
use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

use crate::model::{InstalledApp, InventorySource, Platform};

const BUNDLED: &str = include_str!("../catalog/products.json");
const MAX_CATALOG_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
    Unknown,
}

impl Availability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Unavailable => "Unavailable",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub url: String,
    pub reviewed_on: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformSupport {
    pub platform: Platform,
    /// Availability of the official native product on this OS family only.
    pub availability: Availability,
    pub evidence: Vec<Evidence>,
    /// Requirements are informational. No target device compatibility is evaluated.
    pub compatibility_note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub publisher: String,
    /// Validated in Windows, macOS, Linux order.
    pub platforms: [PlatformSupport; 3],
    match_rules: Vec<MatchRule>,
    /// Optional in schema v1 for backward compatibility with Windows-only catalogs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    native_match_rules: Vec<NativeMatchRule>,
}

impl Product {
    pub fn support(&self, platform: Platform) -> &PlatformSupport {
        &self.platforms[match platform {
            Platform::Windows => 0,
            Platform::Macos => 1,
            Platform::Linux => 2,
        }]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchRule {
    name: String,
    publisher: String,
    evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeMatchRule {
    source: InventorySource,
    native_id: String,
    evidence: Vec<Evidence>,
}

/// `Matched` means source metadata associated with a reviewed catalog entry.
/// Metadata can be forged; this does not verify executable signatures or origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchOutcome {
    Matched(Box<Product>),
    Unknown(UnknownReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    MissingPublisher,
    MissingNativeId,
    UnrecognizedMetadata,
    UnsupportedInventorySource,
    InvalidMetadata,
}

impl UnknownReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::MissingPublisher => "Unknown identity: publisher missing",
            Self::MissingNativeId => "Unknown identity: native identifier missing",
            Self::UnrecognizedMetadata => {
                "Unknown identity: no reviewed source metadata association"
            }
            Self::UnsupportedInventorySource => "Unknown identity: inventory source not reviewed",
            Self::InvalidMetadata => "Unknown identity: invalid inventory metadata",
        }
    }
}

#[derive(Debug)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Invalid catalog: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug)]
pub struct Catalog {
    schema_version: u32,
    products: Vec<Product>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    schema_version: u32,
    products: Vec<Product>,
}

impl Catalog {
    pub fn bundled() -> Result<Self, CatalogError> {
        Self::from_json(BUNDLED)
    }

    pub(crate) fn from_json(json: &str) -> Result<Self, CatalogError> {
        if json.len() > MAX_CATALOG_BYTES {
            return Err(CatalogError("byte limit exceeded".into()));
        }
        let document: CatalogDocument =
            serde_json::from_str(json).map_err(|error| CatalogError(error.to_string()))?;
        let catalog = Self {
            schema_version: document.schema_version,
            products: document.products,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), CatalogError> {
        let fail = |message: &str| Err(CatalogError(message.into()));
        if self.schema_version != 1 || self.products.is_empty() || self.products.len() > 64 {
            return fail("unsupported schema or product count");
        }
        let mut ids = HashSet::new();
        let mut pairs = HashSet::new();
        let mut native_ids = HashSet::new();
        for product in &self.products {
            if !valid_text(&product.id, 64)
                || !product
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !ids.insert(&product.id)
                || !valid_text(&product.name, 128)
                || !valid_text(&product.publisher, 128)
                || product.match_rules.is_empty()
                || product.match_rules.len() > 16
                || product.native_match_rules.len() > 16
            {
                return fail("invalid or duplicate product");
            }
            for rule in &product.match_rules {
                if !valid_text(&rule.name, 512)
                    || !valid_text(&rule.publisher, 512)
                    || !pairs.insert((&rule.name, &rule.publisher))
                    || !valid_evidence(&rule.evidence, true)
                {
                    return fail("invalid, duplicate, or ambiguous match rule");
                }
            }
            for rule in &product.native_match_rules {
                // Flatpak origin currently records only a local remote alias, not trust.
                if rule.source != InventorySource::MacosBundle
                    || !valid_text(&rule.native_id, 256)
                    || !native_ids.insert(&rule.native_id)
                    || !valid_evidence(&rule.evidence, true)
                {
                    return fail("invalid, duplicate, or ambiguous native match rule");
                }
            }
            for (support, expected) in
                product
                    .platforms
                    .iter()
                    .zip([Platform::Windows, Platform::Macos, Platform::Linux])
            {
                if support.platform != expected
                    || !valid_text(&support.compatibility_note, 1024)
                    || !valid_evidence(
                        &support.evidence,
                        support.availability != Availability::Unknown,
                    )
                {
                    return fail("invalid platform support or missing evidence");
                }
            }
        }
        Ok(())
    }

    /// Exact, case-sensitive strings only. Version and registry ID do not prove identity.
    /// No network, package lookup, fuzzy matching, or executable action occurs.
    pub fn match_app(&self, app: &InstalledApp) -> MatchOutcome {
        if !valid_text(&app.name, 512)
            || app
                .publisher
                .as_deref()
                .is_some_and(|publisher| !valid_text(publisher, 512))
            || app
                .native_id
                .as_deref()
                .is_some_and(|id| !valid_text(id, 256))
            || app
                .origin
                .as_deref()
                .is_some_and(|origin| !valid_text(origin, 256))
        {
            return MatchOutcome::Unknown(UnknownReason::InvalidMetadata);
        }
        if app.source == InventorySource::MacosBundle {
            let Some(native_id) = app.native_id.as_deref() else {
                return MatchOutcome::Unknown(UnknownReason::MissingNativeId);
            };
            return match self.products.iter().find(|product| {
                product
                    .native_match_rules
                    .iter()
                    .any(|rule| rule.source == app.source && rule.native_id == native_id)
            }) {
                Some(product) => MatchOutcome::Matched(Box::new(product.clone())),
                None => MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata),
            };
        }
        if app.source != InventorySource::WindowsRegistry {
            return MatchOutcome::Unknown(UnknownReason::UnsupportedInventorySource);
        }
        let Some(publisher) = app.publisher.as_deref() else {
            return MatchOutcome::Unknown(UnknownReason::MissingPublisher);
        };
        match self.products.iter().find(|product| {
            product
                .match_rules
                .iter()
                .any(|rule| rule.name == app.name && rule.publisher == publisher)
        }) {
            Some(product) => MatchOutcome::Matched(Box::new(product.clone())),
            None => MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata),
        }
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= max
        && !value.chars().any(char::is_control)
}

fn valid_evidence(evidence: &[Evidence], required: bool) -> bool {
    (!required || !evidence.is_empty())
        && evidence.len() <= 8
        && evidence.iter().all(|item| {
            valid_https_url(&item.url)
                && valid_date(&item.reviewed_on)
                && valid_text(&item.summary, 512)
        })
}

// Evidence links are HTTPS pages, never credential-bearing or executable schemes.
fn valid_https_url(url: &str) -> bool {
    if url.len() > 2048
        || !url.is_ascii()
        || url
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control() || c == b'\\')
    {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = date[..4].parse().unwrap_or(0);
    let month: u32 = date[5..7].parse().unwrap_or(0);
    let day: u32 = date[8..].parse().unwrap_or(0);
    let days = match month {
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year >= 2020 && day > 0 && day <= days
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::InstallScope;

    fn app(name: &str, publisher: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: "registry:example".into(),
            name: name.into(),
            publisher: publisher.map(str::to_owned),
            version: Some("1.0".into()),
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: None,
            origin: None,
        }
    }

    #[test]
    fn reviewed_pairs_match_and_expose_official_platform_evidence() {
        let catalog = Catalog::bundled().unwrap();
        for product in &catalog.products {
            for rule in &product.match_rules {
                let MatchOutcome::Matched(found) =
                    catalog.match_app(&app(&rule.name, Some(&rule.publisher)))
                else {
                    panic!("reviewed pair must match");
                };
                assert_eq!(found.id, product.id);
                for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
                    let support = found.support(platform);
                    assert_eq!(support.platform, platform);
                    assert!(!support.evidence.is_empty());
                }
            }
        }
        let MatchOutcome::Matched(npp) =
            catalog.match_app(&app("Notepad++ (64-bit x64)", Some("Notepad++ Team")))
        else {
            panic!();
        };
        assert_eq!(
            npp.support(Platform::Windows).availability,
            Availability::Available
        );
        assert_eq!(
            npp.support(Platform::Macos).availability,
            Availability::Unavailable
        );
        assert_eq!(
            npp.support(Platform::Linux).availability,
            Availability::Unavailable
        );
    }

    #[test]
    fn missing_mismatched_fork_and_edition_metadata_remain_unknown() {
        let catalog = Catalog::bundled().unwrap();
        assert_eq!(
            catalog.match_app(&app("VLC media player", None)),
            MatchOutcome::Unknown(UnknownReason::MissingPublisher)
        );
        for (name, publisher) in [
            ("VLC media player", "Unknown repack"),
            ("VLC", "VideoLAN"),
            ("VLC media player Beta", "VideoLAN"),
            ("vlc media player", "VideoLAN"),
            ("VSCodium", "Microsoft Corporation"),
            ("Code - OSS", "Microsoft Corporation"),
            (
                "Microsoft Visual Studio Code Insiders",
                "Microsoft Corporation",
            ),
            ("Notepad++ Pro", "Notepad++ Team"),
            ("Notepad Next", "Notepad++ Team"),
            ("Notepad++ (x64)", "Notepad++ team (MSI installer)"),
            ("Completely unknown", "Example vendor"),
        ] {
            assert_eq!(
                catalog.match_app(&app(name, Some(publisher))),
                MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata)
            );
        }
        for source in [InventorySource::Manual, InventorySource::Flatpak] {
            let mut observation = app("VLC media player", Some("VideoLAN"));
            observation.source = source;
            assert_eq!(
                catalog.match_app(&observation),
                MatchOutcome::Unknown(UnknownReason::UnsupportedInventorySource)
            );
        }
        assert_eq!(
            catalog.match_app(&app("VLC media player\n", Some("VideoLAN"))),
            MatchOutcome::Unknown(UnknownReason::InvalidMetadata)
        );
    }

    #[test]
    fn rejects_duplicate_products_match_pairs_and_invalid_platforms() {
        let original: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        let mut document = original.clone();
        let product = document["products"][0].clone();
        document["products"].as_array_mut().unwrap().push(product);
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut document = original.clone();
        document["products"][1]["match_rules"][0] =
            document["products"][0]["match_rules"][0].clone();
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut document = original.clone();
        document["products"][0]["platforms"][1]["platform"] = "windows".into();
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut document = original;
        document["schema_version"] = 2.into();
        assert!(Catalog::from_json(&document.to_string()).is_err());
    }

    #[test]
    fn native_identifiers_are_exact_and_bound_to_reviewed_source() {
        let catalog = Catalog::bundled().unwrap();
        let mut native = app("VLC", None);
        native.source = InventorySource::MacosBundle;
        assert_eq!(
            catalog.match_app(&native),
            MatchOutcome::Unknown(UnknownReason::MissingNativeId)
        );
        native.native_id = Some("org.videolan.vlc".into());
        let MatchOutcome::Matched(product) = catalog.match_app(&native) else {
            panic!("reviewed bundle ID must match")
        };
        assert_eq!(product.id, "vlc-media-player");
        for id in [
            "org.videolan.VLC",
            "org.videolan.vlc.beta",
            "com.microsoft.VSCode",
            "com.microsoft.VSCodeInsiders",
            "com.visualstudio.code.oss",
        ] {
            native.native_id = Some(id.into());
            assert_eq!(
                catalog.match_app(&native),
                MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata)
            );
        }
        native.native_id = Some("org.videolan.vlc".into());
        for source in [
            InventorySource::WindowsRegistry,
            InventorySource::Manual,
            InventorySource::Flatpak,
        ] {
            native.source = source;
            assert!(matches!(
                catalog.match_app(&native),
                MatchOutcome::Unknown(_)
            ));
        }
        native.source = InventorySource::MacosBundle;
        native.native_id = Some("org.videolan.vlc\n".into());
        assert_eq!(
            catalog.match_app(&native),
            MatchOutcome::Unknown(UnknownReason::InvalidMetadata)
        );
    }

    #[test]
    fn native_rule_validation_rejects_ambiguity_and_unreviewed_sources() {
        let original: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        for source in ["windows_registry", "manual", "flatpak"] {
            let mut document = original.clone();
            document["products"][1]["native_match_rules"][0]["source"] = source.into();
            assert!(Catalog::from_json(&document.to_string()).is_err());
        }
        let mut document = original.clone();
        document["products"][0]["native_match_rules"] =
            document["products"][1]["native_match_rules"].clone();
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut document = original.clone();
        document["products"][1]["native_match_rules"][0]["evidence"] = serde_json::json!([]);
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut legacy = original;
        for product in legacy["products"].as_array_mut().unwrap() {
            product
                .as_object_mut()
                .unwrap()
                .remove("native_match_rules");
        }
        assert!(Catalog::from_json(&legacy.to_string()).is_ok());
    }

    #[test]
    fn rejects_unsupported_claims_bad_urls_dates_unknown_fields_and_limits() {
        let original: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        for url in [
            "http://vendor.example/",
            "javascript:alert(1)",
            "file:///app.exe",
            "https://user@vendor.example/",
            "https:///no-host",
            "https://vendor.example/\n",
            "https://vendor.example\\other",
        ] {
            let mut document = original.clone();
            document["products"][0]["platforms"][0]["evidence"][0]["url"] = url.into();
            assert!(Catalog::from_json(&document.to_string()).is_err(), "{url}");
        }
        let mut document = original.clone();
        document["products"][0]["platforms"][0]["evidence"] = serde_json::json!([]);
        assert!(Catalog::from_json(&document.to_string()).is_err());
        document["products"][0]["platforms"][0]["availability"] = "unknown".into();
        let catalog = Catalog::from_json(&document.to_string()).unwrap();
        assert_eq!(
            catalog.products[0].support(Platform::Windows).availability,
            Availability::Unknown
        );
        let mut document = original.clone();
        document["products"][0]["platforms"][0]["evidence"][0]["reviewed_on"] = "2026-02-30".into();
        assert!(Catalog::from_json(&document.to_string()).is_err());
        let mut document = original;
        document["products"][0]["installer_command"] = "run".into();
        assert!(Catalog::from_json(&document.to_string()).is_err());
        assert!(Catalog::from_json(&" ".repeat(MAX_CATALOG_BYTES + 1)).is_err());
    }
}
