//! Curated and provider metadata associations, never installation authorization.
use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

use crate::model::{InstalledApp, InventorySource, Platform};

mod providers;
mod winget;

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
    /// Reviewed vendor availability or a listed package for this OS family.
    /// Package listings may be maintained by the community, not the vendor.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winget_id: Option<String>,
    #[serde(skip)]
    pub packages: Vec<PackageReference>,
    #[serde(skip)]
    pub association: AssociationSource,
    /// Validated in Windows, macOS, Linux order.
    pub platforms: [PlatformSupport; 3],
    match_rules: Vec<MatchRule>,
    /// Optional in schema v1 for backward compatibility with Windows-only catalogs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    native_match_rules: Vec<NativeMatchRule>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssociationSource {
    #[default]
    Curated,
    WingetProductCode,
    WingetMetadata,
    HomebrewBundle,
    FlathubId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageReference {
    pub manager: String,
    pub id: String,
    pub url: String,
}

impl AssociationSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Curated => "Curated catalog metadata",
            Self::WingetProductCode => "WinGet product identifier",
            Self::WingetMetadata => "WinGet name and publisher metadata",
            Self::HomebrewBundle => "Homebrew bundle identifier",
            Self::FlathubId => "Flathub application identifier",
        }
    }
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

/// `Matched` means metadata associated with a curated or provider catalog entry.
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
            Self::UnrecognizedMetadata => "Unknown identity: no catalog metadata association",
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
    winget: Option<&'static winget::Index>,
    providers: Option<&'static providers::Index>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    schema_version: u32,
    products: Vec<Product>,
}

impl Catalog {
    pub fn bundled() -> Result<Self, CatalogError> {
        let mut catalog = Self::from_json(BUNDLED)?;
        catalog.winget = Some(winget::bundled().map_err(CatalogError)?);
        catalog.providers = Some(providers::bundled().map_err(CatalogError)?);
        catalog
            .providers
            .unwrap()
            .validate_links(&catalog.products, catalog.winget.unwrap())
            .map_err(CatalogError)?;
        Ok(catalog)
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
            winget: None,
            providers: None,
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
        let mut winget_ids = HashSet::new();
        for product in &self.products {
            if !valid_text(&product.id, 64)
                || !product
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !ids.insert(&product.id)
                || !valid_text(&product.name, 128)
                || !valid_text(&product.publisher, 128)
                || product
                    .winget_id
                    .as_deref()
                    .is_some_and(|id| !winget::valid_id(id) || !winget_ids.insert(id))
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

    /// Exact curated metadata, then unambiguous package-manager identifiers.
    /// Registry keys use ASCII case-insensitive comparison. No network or actions.
    pub fn match_app(&self, app: &InstalledApp) -> MatchOutcome {
        let mut outcome = self.match_metadata(app);
        if let MatchOutcome::Matched(product) = &mut outcome {
            if let Some(index) = self.providers {
                index.enrich(product);
            }
            if let Some(id) = &product.winget_id {
                let url = winget::package_url(id);
                product.packages.push(PackageReference {
                    manager: "WinGet".into(),
                    id: id.clone(),
                    url: url.clone(),
                });
                let support = &mut product.platforms[0];
                if support.availability == Availability::Unknown
                    && let Some(index) = self.winget
                {
                    support.availability = Availability::Available;
                    support.evidence.push(Evidence { url, reviewed_on: index.generated_on().to_owned(), summary: "Exact package listed in Microsoft's WinGet community catalog. This does not verify the installed binary or target requirements.".into() });
                    support.compatibility_note = "WinGet package exists for Windows. Review architecture, Windows release, publisher and installer requirements before use.".into();
                }
            }
        }
        outcome
    }

    fn match_metadata(&self, app: &InstalledApp) -> MatchOutcome {
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
                None => self.match_provider(app),
            };
        }
        if app.source == InventorySource::Flatpak {
            return self.match_provider(app);
        }
        if app.source != InventorySource::WindowsRegistry {
            return MatchOutcome::Unknown(UnknownReason::UnsupportedInventorySource);
        }
        let Some(publisher) = app.publisher.as_deref() else {
            return match self.match_winget(app) {
                matched @ MatchOutcome::Matched(_) => matched,
                MatchOutcome::Unknown(_) => MatchOutcome::Unknown(UnknownReason::MissingPublisher),
            };
        };
        match self.products.iter().find(|product| {
            product
                .match_rules
                .iter()
                .any(|rule| rule.name == app.name && rule.publisher == publisher)
        }) {
            Some(product) => MatchOutcome::Matched(Box::new(product.clone())),
            None => self.match_winget(app),
        }
    }

    fn match_winget(&self, app: &InstalledApp) -> MatchOutcome {
        let by_code = self
            .winget
            .and_then(|index| index.find(app.native_id.as_deref()?));
        let association = if by_code.is_some() {
            AssociationSource::WingetProductCode
        } else {
            AssociationSource::WingetMetadata
        };
        let Some((id, name)) = by_code.or_else(|| {
            self.winget
                .and_then(|index| index.find_metadata(&app.name, app.publisher.as_deref()?))
        }) else {
            return MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata);
        };
        let mut product = if let Some(curated) = self
            .products
            .iter()
            .find(|p| p.winget_id.as_deref() == Some(id))
        {
            curated.clone()
        } else {
            Product {
                id: format!("winget:{id}"),
                name: name.into(),
                publisher: app.publisher.clone().unwrap_or_else(|| "Publisher unverified".into()),
                winget_id: Some(id.into()),
                packages: Vec::new(),
                association: AssociationSource::WingetProductCode,
                platforms: [Platform::Windows, Platform::Macos, Platform::Linux].map(|platform| {
                    PlatformSupport {
                        platform,
                        availability: Availability::Unknown,
                        evidence: Vec::new(),
                        compatibility_note: "WinGet metadata association only. No evidence for this OS family or device requirements.".into(),
                    }
                }),
                match_rules: Vec::new(),
                native_match_rules: Vec::new(),
            }
        };
        product.association = association;
        MatchOutcome::Matched(Box::new(product))
    }

    fn match_provider(&self, app: &InstalledApp) -> MatchOutcome {
        let Some(id) = app.native_id.as_deref() else {
            return MatchOutcome::Unknown(UnknownReason::MissingNativeId);
        };
        let id = if app.source == InventorySource::Flatpak {
            let parts: Vec<_> = id.split('/').collect();
            if parts.len() != 3 || parts[2] != "stable" || !winget::valid_id(parts[1]) {
                return MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata);
            }
            parts[0]
        } else {
            id
        };
        let Some(package) = self.providers.and_then(|index| index.find(app.source, id)) else {
            return MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata);
        };
        let mut product = self.products.iter().find(|product| product.id == package.canonical_id).cloned().unwrap_or_else(|| Product {
            id: package.canonical_id.clone(), name: package.name.clone(),
            publisher: app.publisher.clone().unwrap_or_else(|| "Publisher unverified".into()),
            winget_id: package.canonical_id.strip_prefix("winget:").map(str::to_owned),
            packages: Vec::new(), association: AssociationSource::Curated,
            platforms: [Platform::Windows, Platform::Macos, Platform::Linux].map(|platform| PlatformSupport {
                platform, availability: Availability::Unknown, evidence: Vec::new(),
                compatibility_note: "No package or vendor evidence for this OS. Absence does not establish incompatibility.".into(),
            }), match_rules: Vec::new(), native_match_rules: Vec::new(),
        });
        product.association = if app.source == InventorySource::MacosBundle {
            AssociationSource::HomebrewBundle
        } else {
            AssociationSource::FlathubId
        };
        MatchOutcome::Matched(Box::new(product))
    }
}

/// Share only catalog-recognized registration identifiers, never arbitrary registry keys.
#[cfg(any(windows, test))]
pub(crate) fn windows_native_id(key: &str) -> Option<String> {
    if !valid_text(key, 256) {
        return None;
    }
    winget::bundled().ok()?.find(key).map(|_| key.to_owned())
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
    fn winget_product_codes_cover_versioned_apps_without_inventing_os_support() {
        let catalog = Catalog::bundled().unwrap();
        let mut observation = app("7-Zip 25.01 (x64)", Some("Igor Pavlov"));
        observation.native_id = Some("7-Zip".into());
        let MatchOutcome::Matched(product) = catalog.match_app(&observation) else {
            panic!("WinGet's exact uninstall identifier must associate versioned names");
        };
        assert_eq!(product.id, "winget:7zip.7zip");
        assert_eq!(
            product.support(Platform::Windows).availability,
            Availability::Available
        );
        for platform in [Platform::Macos, Platform::Linux] {
            assert_eq!(
                product.support(platform).availability,
                Availability::Unknown
            );
        }
        observation.native_id = Some("7-Zip Beta".into());
        assert!(matches!(
            catalog.match_app(&observation),
            MatchOutcome::Unknown(_)
        ));
        observation.native_id = Some("7-Zip".into());
        observation.source = InventorySource::Manual;
        assert!(matches!(
            catalog.match_app(&observation),
            MatchOutcome::Unknown(_)
        ));
    }

    #[test]
    fn winget_codes_reuse_curated_identity_and_availability() {
        let catalog = Catalog::bundled().unwrap();
        let mut observation = app("OBS Studio (different display label)", Some("OBS Project"));
        observation.native_id = Some("OBS Studio".into());
        let MatchOutcome::Matched(product) = catalog.match_app(&observation) else {
            panic!("WinGet code should resolve to existing curated product");
        };
        assert_eq!(product.id, "obs-studio");
        assert_eq!(
            product.support(Platform::Macos).availability,
            Availability::Available
        );
    }

    #[test]
    fn package_managers_resolve_native_ids_to_the_same_cross_platform_product() {
        let catalog = Catalog::bundled().unwrap();
        for (source, native_id, id) in [
            (
                InventorySource::MacosBundle,
                "com.microsoft.VSCode",
                "visual-studio-code",
            ),
            (
                InventorySource::Flatpak,
                "com.visualstudio.code/x86_64/stable",
                "visual-studio-code",
            ),
            (
                InventorySource::Flatpak,
                "org.videolan.VLC/x86_64/stable",
                "vlc-media-player",
            ),
        ] {
            let mut observation = app("Observed application label", None);
            observation.source = source;
            observation.native_id = Some(native_id.into());
            let MatchOutcome::Matched(product) = catalog.match_app(&observation) else {
                panic!("provider metadata must recognize {native_id}");
            };
            assert_eq!(product.id, id);
            assert_eq!(
                product.support(Platform::Windows).availability,
                Availability::Available
            );
        }
    }

    #[test]
    fn provider_only_apps_have_evidence_without_inventing_other_os_support() {
        let catalog = Catalog::bundled().unwrap();
        let mut observation = app("GNOME Calculator", None);
        observation.source = InventorySource::Flatpak;
        observation.native_id = Some("org.gnome.Calculator/x86_64/stable".into());
        let MatchOutcome::Matched(product) = catalog.match_app(&observation) else {
            panic!("Flathub application must be recognized");
        };
        assert_eq!(
            product.support(Platform::Linux).availability,
            Availability::Available
        );
        assert!(!product.support(Platform::Linux).evidence.is_empty());
        assert_eq!(
            product.support(Platform::Windows).availability,
            Availability::Unknown
        );
        assert_eq!(
            product.support(Platform::Macos).availability,
            Availability::Unknown
        );
        observation.native_id = Some("org.gnome.Calculator.beta".into());
        assert!(matches!(
            catalog.match_app(&observation),
            MatchOutcome::Unknown(_)
        ));
    }

    #[test]
    fn windows_metadata_cross_references_packages_and_keeps_editions_separate() {
        let catalog = Catalog::bundled().unwrap();
        let MatchOutcome::Matched(bitwarden) =
            catalog.match_app(&app("Bitwarden", Some("Bitwarden Inc.")))
        else {
            panic!("WinGet name and publisher must recognize Bitwarden without an uninstall code");
        };
        assert_eq!(bitwarden.id, "winget:Bitwarden.Bitwarden");
        assert_eq!(bitwarden.association, AssociationSource::WingetMetadata);
        assert!(
            bitwarden
                .packages
                .iter()
                .any(|p| p.manager == "Homebrew" && p.id == "bitwarden")
        );
        assert!(
            bitwarden
                .packages
                .iter()
                .any(|p| p.manager == "Flathub" && p.id == "com.bitwarden.desktop")
        );
        for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
            assert_eq!(
                bitwarden.support(platform).availability,
                Availability::Available
            );
        }
        let mut linux = app("Local Bitwarden label", None);
        linux.source = InventorySource::Flatpak;
        linux.native_id = Some("com.bitwarden.desktop/x86_64/stable".into());
        let MatchOutcome::Matched(counterpart) = catalog.match_app(&linux) else {
            panic!("Flatpak counterpart");
        };
        assert_eq!(counterpart.id, bitwarden.id);
        let MatchOutcome::Matched(insiders) = catalog.match_app(&app(
            "Microsoft Visual Studio Code Insiders",
            Some("Microsoft Corporation"),
        )) else {
            panic!("Insiders should have its own package identity");
        };
        assert_eq!(insiders.id, "winget:Microsoft.VisualStudioCode.Insiders");
        assert_eq!(
            insiders.support(Platform::Linux).availability,
            Availability::Unknown
        );
        linux.native_id = Some("io.github.shiftey.Desktop/x86_64/stable".into());
        let MatchOutcome::Matched(fork) = catalog.match_app(&linux) else {
            panic!("Separate community package");
        };
        assert_ne!(fork.id, "github-desktop");
        assert_eq!(
            fork.support(Platform::Windows).availability,
            Availability::Unknown
        );
    }

    #[test]
    fn common_windows_apps_match_without_accepting_repackages_or_other_channels() {
        let catalog = Catalog::bundled().unwrap();
        // Representative uninstall metadata, including Firefox's literal architecture/locale.
        for (name, publisher, id) in [
            ("Brave", "Brave Software Inc", "brave"),
            ("Docker Desktop", "Docker Inc.", "docker-desktop"),
            ("Mozilla Firefox (x64 en-US)", "Mozilla", "firefox"),
            ("Git", "The Git Development Community", "git"),
            ("OBS Studio", "OBS Project", "obs-studio"),
            ("Steam", "Valve Corporation", "steam"),
            ("Google Chrome", "Google LLC", "google-chrome"),
            ("Microsoft Edge", "Microsoft Corporation", "microsoft-edge"),
            ("Discord", "Discord Inc.", "discord"),
            ("Spotify", "Spotify AB", "spotify"),
            ("Obsidian", "Obsidian", "obsidian"),
            ("GitHub Desktop", "GitHub, Inc.", "github-desktop"),
        ] {
            let MatchOutcome::Matched(product) = catalog.match_app(&app(name, Some(publisher)))
            else {
                panic!("expected reviewed match for {name}");
            };
            assert_eq!(product.id, id);
            assert_eq!(
                catalog.match_app(&app(name, Some("Unreviewed publisher"))),
                MatchOutcome::Unknown(UnknownReason::UnrecognizedMetadata)
            );
            if let MatchOutcome::Matched(beta) =
                catalog.match_app(&app(&format!("{name} Beta"), Some(publisher)))
            {
                assert_ne!(
                    beta.id, id,
                    "Beta must never use the stable product identity"
                );
            }
        }
        let MatchOutcome::Matched(desktop) =
            catalog.match_app(&app("GitHub Desktop", Some("GitHub, Inc.")))
        else {
            panic!("expected GitHub Desktop match");
        };
        assert_eq!(
            desktop.support(Platform::Linux).availability,
            Availability::Unavailable
        );
        for (name, publisher) in [
            ("Mozilla Maintenance Service", "Mozilla"),
            ("Mozilla Firefox ESR (x64 en-US)", "Mozilla"),
            ("Brave Nightly", "Brave Software Inc"),
            ("Discord Canary", "Discord Inc."),
            ("Microsoft Edge WebView2 Runtime", "Microsoft Corporation"),
            ("GitHub Desktop Deployment Tool", "GitHub, Inc."),
        ] {
            if let MatchOutcome::Matched(other) = catalog.match_app(&app(name, Some(publisher))) {
                assert!(
                    !catalog.products.iter().any(|stable| stable.id == other.id),
                    "A separate edition or maintenance component must not use a stable product identity"
                );
            }
        }
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
            ("VSCodium", "Microsoft Corporation"),
            ("Code - OSS", "Microsoft Corporation"),
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
                MatchOutcome::Unknown(if source == InventorySource::Manual {
                    UnknownReason::UnsupportedInventorySource
                } else {
                    UnknownReason::MissingNativeId
                })
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
