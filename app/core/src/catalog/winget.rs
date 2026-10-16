//! Offline, generated WinGet metadata. Never installation authorization.
use std::{collections::HashSet, sync::OnceLock};

use serde::Deserialize;

use super::{valid_date, valid_text};

const SOURCE_URL: &str = "https://cdn.winget.microsoft.com/cache/source2.msix";
const BUNDLED: &str = include_str!("../../catalog/winget-index.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Index {
    schema_version: u32,
    source_url: String,
    source_sha256: String,
    source_updated_on: String,
    generated_on: String,
    packages: Vec<(String, String)>,
    codes: Vec<(String, usize)>,
    pairs: Vec<(String, String, usize)>,
}

impl Index {
    pub(super) fn contains_id(&self, id: &str) -> bool {
        self.packages.iter().any(|(candidate, _)| candidate == id)
    }

    pub(super) fn generated_on(&self) -> &str {
        &self.generated_on
    }
    fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > 32 * 1024 * 1024 {
            return Err("WinGet index byte limit exceeded".into());
        }
        let index: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        if index.schema_version != 2
            || index.source_url != SOURCE_URL
            || index.source_sha256.len() != 64
            || !index.source_sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || !valid_date(&index.source_updated_on)
            || !valid_date(&index.generated_on)
            || index.packages.is_empty()
            || index.packages.len() > 100_000
            || index.codes.is_empty()
            || index.codes.len() > 500_000
            || index.pairs.len() > 500_000
        {
            return Err("Invalid WinGet index metadata or limits".into());
        }
        let mut ids = HashSet::new();
        for (id, name) in &index.packages {
            if !valid_id(id) || !valid_text(name, 512) || !ids.insert(id) {
                return Err("Invalid or duplicate WinGet package".into());
            }
        }
        for (position, (code, package)) in index.codes.iter().enumerate() {
            if !valid_text(code, 256)
                || !code.is_ascii()
                || code.bytes().any(|c| c.is_ascii_uppercase())
                || *package >= index.packages.len()
                || (position > 0 && index.codes[position - 1].0 >= *code)
            {
                return Err("Invalid, duplicate or unsorted WinGet product code".into());
            }
        }
        for (position, (name, publisher, package)) in index.pairs.iter().enumerate() {
            if metadata_key(name).as_deref() != Some(name)
                || metadata_key(publisher).as_deref() != Some(publisher)
                || *package >= index.packages.len()
                || (position > 0
                    && (&index.pairs[position - 1].0, &index.pairs[position - 1].1)
                        >= (name, publisher))
            {
                return Err("Invalid, duplicate or unsorted WinGet metadata pair".into());
            }
        }
        Ok(index)
    }

    pub(super) fn find_metadata(&self, name: &str, publisher: &str) -> Option<(&str, &str)> {
        let name = metadata_key(name)?;
        let mut words: Vec<_> = publisher
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        while words.last().is_some_and(|word| {
            [
                "inc",
                "incorporated",
                "corp",
                "corporation",
                "llc",
                "ltd",
                "limited",
                "gmbh",
                "co",
                "company",
            ]
            .iter()
            .any(|suffix| word.eq_ignore_ascii_case(suffix))
        }) {
            words.pop();
        }
        // Preserve all name/version/channel words. Non-ASCII input abstains.
        if !publisher.is_ascii() {
            return None;
        }
        let publisher = metadata_key(&words.join(" "))?;
        let position = self
            .pairs
            .binary_search_by(|(n, p, _)| (n, p).cmp(&(&name, &publisher)))
            .ok()?;
        let (id, display_name) = &self.packages[self.pairs[position].2];
        // Historical source aliases sometimes point to another major version or
        // sibling component. Require the current canonical display name as well.
        if metadata_key(display_name)? != name {
            return None;
        }
        // The source contains historical publisher aliases for some forks. Require
        // the package's publisher namespace to corroborate the observed publisher.
        if metadata_key(id.split('.').next()?)? != publisher {
            return None;
        }
        Some((id, display_name))
    }

    pub(super) fn find(&self, code: &str) -> Option<(&str, &str)> {
        let code = code.to_ascii_lowercase();
        let position = self
            .codes
            .binary_search_by(|(key, _)| key.cmp(&code))
            .ok()?;
        let (id, name) = &self.packages[self.codes[position].1];
        Some((id, name))
    }
}

fn metadata_key(value: &str) -> Option<String> {
    if !valid_text(value, 512) || !value.is_ascii() {
        return None;
    }
    let key: String = value
        .bytes()
        .filter(|c| c.is_ascii_alphanumeric() || b"+#@".contains(c))
        .map(|c| c.to_ascii_lowercase() as char)
        .collect();
    (!key.is_empty()).then_some(key)
}

pub(super) fn package_url(id: &str) -> String {
    format!(
        "https://github.com/microsoft/winget-pkgs/tree/master/manifests/{}/{}",
        id[..1].to_ascii_lowercase(),
        id.replace('.', "/")
    )
}

pub(super) fn valid_id(id: &str) -> bool {
    valid_text(id, 256)
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
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

    fn fixture() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2, "source_url": SOURCE_URL,
            "source_sha256": "0".repeat(64), "source_updated_on": "2026-10-15",
            "generated_on": "2026-10-15", "packages": [["Vendor.App", "Example"]],
            "codes": [["example_is1", 0]], "pairs": [["example", "vendor", 0]]
        })
    }

    #[test]
    fn exact_codes_allow_registry_case_but_never_suffixes_or_partial_names() {
        let index = Index::from_json(&fixture().to_string()).unwrap();
        assert_eq!(index.find("Example_is1"), Some(("Vendor.App", "Example")));
        assert_eq!(index.find("example_is1 Beta"), None);
        assert_eq!(index.find("example"), None);
    }

    #[test]
    fn rejects_ambiguity_bad_indices_ordering_and_untrusted_metadata() {
        for codes in [
            serde_json::json!([["example_is1", 0], ["example_is1", 0]]),
            serde_json::json!([["z", 0], ["a", 0]]),
            serde_json::json!([["example_is1", 1]]),
            serde_json::json!([["Example_is1", 0]]),
            serde_json::json!([["bad\ncode", 0]]),
        ] {
            let mut json = fixture();
            json["codes"] = codes;
            assert!(Index::from_json(&json.to_string()).is_err());
        }
        for (field, value) in [
            ("schema_version", serde_json::json!(3)),
            (
                "source_url",
                serde_json::json!("https://other.example/index"),
            ),
            ("source_sha256", serde_json::json!("not-a-hash")),
            ("generated_on", serde_json::json!("2026-02-30")),
            ("installer_command", serde_json::json!("do not execute")),
        ] {
            let mut json = fixture();
            json[field] = value;
            assert!(Index::from_json(&json.to_string()).is_err());
        }
    }

    #[test]
    fn metadata_requires_both_fields_and_preserves_channels_and_versions() {
        let index = Index::from_json(&fixture().to_string()).unwrap();
        assert_eq!(
            index.find_metadata("Example", "Vendor Inc."),
            Some(("Vendor.App", "Example"))
        );
        for (name, publisher) in [
            ("Example Beta", "Vendor"),
            ("Example 2.0", "Vendor"),
            ("Example", "Other Vendor"),
            ("Example", ""),
            ("Example+", "Vendor"),
        ] {
            assert_eq!(index.find_metadata(name, publisher), None);
        }
    }

    #[test]
    fn historical_aliases_cannot_reassign_a_major_version_or_component() {
        let index = bundled().unwrap();
        assert_eq!(index.find_metadata("AdoptOpenJDK 10", "AdoptOpenJDK"), None);
        assert_eq!(index.find_metadata("AdoptOpenJDK 7", "AdoptOpenJDK"), None);
        assert_eq!(
            index.find_metadata("Autodesk Identity Manager", "Autodesk"),
            None
        );
    }
}
