//! Bounded, offline package metadata. Never verifies binaries or authorizes installs.
use super::{
    Availability, Evidence, PackageReference, Product, valid_date, valid_https_url, valid_text,
    winget,
};
use crate::model::{InventorySource, Platform};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

const BUNDLED: &str = include_str!("../../catalog/provider-index.json");
const BREW_URL: &str = "https://formulae.brew.sh/api/cask.json";
const FLATHUB_URL: &str = "https://dl.flathub.org/repo/appstream/x86_64/appstream.xml.gz";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum Provider {
    Homebrew,
    Flathub,
}

impl Provider {
    fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Flathub => "Flathub",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Homebrew => "homebrew:",
            Self::Flathub => "flathub:",
        }
    }
    fn platform(self) -> Platform {
        match self {
            Self::Homebrew => Platform::Macos,
            Self::Flathub => Platform::Linux,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    provider: Provider,
    url: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Package {
    provider: Provider,
    id: String,
    pub(super) name: String,
    names: Vec<String>,
    homepage: Option<String>,
    native_ids: Vec<String>,
    pub(super) canonical_id: String,
}

impl Package {
    fn url(&self) -> String {
        match self.provider {
            Provider::Homebrew => format!("https://formulae.brew.sh/cask/{}", self.id),
            Provider::Flathub => format!("https://flathub.org/apps/{}", self.id),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    generated_on: String,
    sources: Vec<Source>,
    packages: Vec<Package>,
}

#[derive(Debug)]
pub(super) struct Index {
    generated_on: String,
    packages: Vec<Package>,
    native: HashMap<(Provider, String), usize>,
    groups: HashMap<String, Vec<usize>>,
}

impl Index {
    fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > 16 * 1024 * 1024 {
            return Err("Provider index byte limit exceeded".into());
        }
        let document: Document = serde_json::from_str(json).map_err(|error| error.to_string())?;
        if document.schema_version != 1
            || !valid_date(&document.generated_on)
            || document.sources.len() != 2
            || document.packages.is_empty()
            || document.packages.len() > 30_000
        {
            return Err("Invalid provider index metadata or count".into());
        }
        let mut sources = HashSet::new();
        for source in &document.sources {
            let expected = match source.provider {
                Provider::Homebrew => BREW_URL,
                Provider::Flathub => FLATHUB_URL,
            };
            if source.url != expected
                || !sources.insert(source.provider)
                || source.sha256.len() != 64
                || !source.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Unrecognized provider source or digest".into());
            }
        }
        let mut index = Self {
            generated_on: document.generated_on,
            packages: document.packages,
            native: HashMap::new(),
            groups: HashMap::new(),
        };
        let mut keys = HashSet::new();
        for (position, package) in index.packages.iter().enumerate() {
            if !valid_text(&package.id, 256)
                || !package
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-+@".contains(&c))
                || !keys.insert((package.provider, package.id.as_str()))
                || !valid_text(&package.name, 512)
                || package.names.is_empty()
                || package.names.len() > 64
                || !package.names.iter().all(|name| valid_text(name, 512))
                || package
                    .homepage
                    .as_deref()
                    .is_some_and(|url| !valid_https_url(url))
                || !valid_text(&package.canonical_id, 256)
                || package.native_ids.len() > 64
            {
                return Err("Invalid provider package".into());
            }
            for native in &package.native_ids {
                if !winget::valid_id(native)
                    || !native.contains('.')
                    || index
                        .native
                        .insert((package.provider, native.clone()), position)
                        .is_some()
                {
                    return Err("Invalid or ambiguous provider native identifier".into());
                }
            }
            index
                .groups
                .entry(package.canonical_id.clone())
                .or_default()
                .push(position);
        }
        Ok(index)
    }

    pub(super) fn validate_links(
        &self,
        products: &[Product],
        winget: &winget::Index,
    ) -> Result<(), String> {
        let package_keys: HashSet<_> = self
            .packages
            .iter()
            .map(|p| format!("{}{}", p.provider.prefix(), p.id))
            .collect();
        for canonical in self.groups.keys() {
            if products.iter().any(|p| p.id == *canonical) {
                continue;
            }
            if let Some(id) = canonical.strip_prefix("winget:") {
                if winget.contains_id(id) {
                    continue;
                }
            } else if package_keys.contains(canonical) {
                continue;
            }
            return Err("Provider link references an unknown product".into());
        }
        Ok(())
    }

    pub(super) fn find(&self, source: InventorySource, native_id: &str) -> Option<&Package> {
        let provider = match source {
            InventorySource::MacosBundle => Provider::Homebrew,
            InventorySource::Flatpak => Provider::Flathub,
            _ => return None,
        };
        self.native
            .get(&(provider, native_id.to_owned()))
            .map(|position| &self.packages[*position])
    }

    pub(super) fn enrich(&self, product: &mut Product) {
        let Some(positions) = self.groups.get(&product.id) else {
            return;
        };
        for position in positions {
            let package = &self.packages[*position];
            let url = package.url();
            product.packages.push(PackageReference {
                manager: package.provider.label().into(),
                id: package.id.clone(),
                url: url.clone(),
            });
            let support = &mut product.platforms[match package.provider.platform() {
                Platform::Windows => 0,
                Platform::Macos => 1,
                Platform::Linux => 2,
            }];
            // Reviewed vendor restrictions take precedence over community packages.
            if support.availability == Availability::Unknown {
                support.availability = Availability::Available;
                support.evidence = vec![Evidence {
                    url,
                    reviewed_on: self.generated_on.clone(),
                    summary: format!(
                        "Package listed in {}. Community packaging may differ from the vendor distribution; installed binary identity is not verified.",
                        package.provider.label()
                    ),
                }];
                support.compatibility_note = format!(
                    "{} package listing, snapshot {}. The Flathub snapshot covers x86_64 stable packages. Review CPU architecture, OS version, distribution, licensing and package origin before use. A listing is not proof of official vendor support.",
                    package.provider.label(),
                    self.generated_on
                );
            }
        }
    }
}

pub(super) fn bundled() -> Result<&'static Index, String> {
    static INDEX: OnceLock<Result<Index, String>> = OnceLock::new();
    INDEX
        .get_or_init(|| Index::from_json(BUNDLED))
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_index_is_bounded_and_resolves_exact_source_specific_ids() {
        let index = bundled().unwrap();
        assert_eq!(
            index
                .find(InventorySource::Flatpak, "org.videolan.VLC")
                .unwrap()
                .canonical_id,
            "vlc-media-player"
        );
        assert!(
            index
                .find(InventorySource::MacosBundle, "org.videolan.VLC")
                .is_none()
        );
        assert!(
            index
                .find(InventorySource::Flatpak, "org.videolan.vlc")
                .is_none()
        );
        assert!(
            index
                .find(InventorySource::Manual, "org.videolan.VLC")
                .is_none()
        );
    }

    #[test]
    fn malformed_sources_duplicate_native_ids_and_unknown_fields_are_rejected() {
        let original: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        for mutation in 0..4 {
            let mut document = original.clone();
            match mutation {
                0 => document["sources"][0]["url"] = "https://untrusted.example/index".into(),
                1 => document["generated_on"] = "2026-02-30".into(),
                2 => document["packages"][0]["installer_command"] = "unexpected".into(),
                _ => {
                    document["packages"][0]["native_ids"] =
                        serde_json::json!(["org.example.Duplicate"]);
                    document["packages"][1]["native_ids"] =
                        serde_json::json!(["org.example.Duplicate"]);
                }
            }
            assert!(Index::from_json(&document.to_string()).is_err());
        }
    }
}
