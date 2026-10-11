//! Read-only, bounded macOS application-bundle inventory.
use crate::{
    inventory::InventoryError,
    model::{DeviceInfo, InventorySnapshot},
};

pub fn scan(device: DeviceInfo) -> Result<InventorySnapshot, InventoryError> {
    device.validate().map_err(InventoryError::InvalidDevice)?;
    #[cfg(target_os = "macos")]
    {
        if device.platform != crate::model::Platform::Macos {
            return Err(InventoryError::InvalidDevice(
                "Device platform does not match macOS scanner",
            ));
        }
        let mut roots = vec![
            bundles::Root::new(
                "/Applications",
                crate::model::InstallScope::Machine,
                "machine applications",
            ),
            bundles::Root::new(
                "/System/Applications",
                crate::model::InstallScope::Machine,
                "system applications",
            ),
        ];
        let mut warnings = Vec::new();
        match std::env::var_os("HOME").map(std::path::PathBuf::from) {
            Some(home) if home.is_absolute() => roots.push(bundles::Root::new(
                home.join("Applications"),
                crate::model::InstallScope::User,
                "current-user applications",
            )),
            _ => warnings.push(
                "Current-user application source unavailable: home directory is unknown".into(),
            ),
        }
        bundles::scan_roots(device, &roots, warnings)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(InventoryError::UnsupportedPlatform)
    }
}

#[cfg(any(target_os = "macos", test))]
mod bundles {
    use super::*;
    use crate::model::{InstallScope, InstalledApp, InventorySource};
    use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
    use std::{
        collections::HashSet,
        fmt, fs,
        io::{Cursor, Read},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    const MAX_ENTRIES: usize = 40_000;
    const MAX_APPS: usize = 10_000;
    const MAX_DIRECTORY_DEPTH: usize = 2;
    const MAX_PLIST_BYTES: usize = 256 * 1024;
    const MAX_KEYS: usize = 1024;
    const MAX_VALUES: usize = 4096;
    const MAX_PLIST_DEPTH: usize = 16;
    const MAX_TEXT_BYTES: usize = 16 * 1024;
    const COVERAGE: &str = "Coverage: macOS .app bundles in machine, system, and current-user application directories, with at most two nested directory levels. Linked entries, embedded apps, other users, other locations, and localized names are excluded. Metadata does not verify publisher, signatures, catalog identity, or update availability.";

    pub(super) struct Root {
        path: PathBuf,
        scope: InstallScope,
        label: &'static str,
    }

    impl Root {
        pub(super) fn new(
            path: impl Into<PathBuf>,
            scope: InstallScope,
            label: &'static str,
        ) -> Self {
            Self {
                path: path.into(),
                scope,
                label,
            }
        }
    }

    fn warn(warnings: &mut Vec<String>, message: impl Into<String>) {
        if warnings.len() < 63 {
            warnings.push(message.into());
        } else if warnings.len() == 63 {
            warnings.push("Additional macOS inventory warnings suppressed".into());
        }
    }

    fn linked(metadata: &fs::Metadata) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        }
        #[cfg(not(windows))]
        {
            metadata.file_type().is_symlink()
        }
    }

    // Inspect every component before using an absolute source; do not accept a
    // source whose parent is a link either. Filesystem mutation can still race.
    fn unlinked(path: &Path) -> std::io::Result<bool> {
        for ancestor in path.ancestors() {
            if ancestor.as_os_str().is_empty() {
                continue;
            }
            if linked(&fs::symlink_metadata(ancestor)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[derive(Default)]
    struct Metadata {
        display_name: Option<String>,
        name: Option<String>,
        identifier: Option<String>,
        version: Option<String>,
        build: Option<String>,
    }

    fn text(value: Option<String>, max: usize) -> Result<Option<String>, &'static str> {
        let Some(value) = value else { return Ok(None) };
        if value.len() > max || value.chars().any(char::is_control) {
            return Err("Bundle metadata contains invalid or oversized text");
        }
        let trimmed = value.trim();
        Ok((!trimmed.is_empty()).then(|| trimmed.to_owned()))
    }

    #[derive(Default)]
    struct Budget {
        keys: usize,
        values: usize,
    }

    // Deserialize through a bounded visitor instead of allocating an arbitrary
    // Value tree: small binary plists can repeatedly reference the same objects.
    struct Seed<'a> {
        budget: &'a mut Budget,
        depth: usize,
    }
    #[derive(Default)]
    struct Parsed {
        text: Option<String>,
        metadata: Option<Metadata>,
    }

    impl<'de> DeserializeSeed<'de> for Seed<'_> {
        type Value = Parsed;
        fn deserialize<D: de::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Parsed, D::Error> {
            self.budget.values += 1;
            if self.budget.values > MAX_VALUES || self.depth > MAX_PLIST_DEPTH {
                return Err(de::Error::custom(
                    "Bundle property list exceeds structural limits",
                ));
            }
            deserializer.deserialize_any(self)
        }
    }

    impl<'de> Visitor<'de> for Seed<'_> {
        type Value = Parsed;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded property-list data")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Parsed, E> {
            if value.len() > MAX_TEXT_BYTES {
                return Err(E::custom("Bundle property list contains oversized text"));
            }
            Ok(Parsed {
                text: Some(value.to_owned()),
                metadata: None,
            })
        }
        fn visit_string<E: de::Error>(self, value: String) -> Result<Parsed, E> {
            self.visit_str(&value)
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_unit<E: de::Error>(self) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<Parsed, E> {
            Ok(Parsed::default())
        }
        fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Parsed, E> {
            self.visit_bytes(&value)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Parsed, A::Error> {
            while sequence
                .next_element_seed(Seed {
                    budget: self.budget,
                    depth: self.depth + 1,
                })?
                .is_some()
            {}
            Ok(Parsed::default())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Parsed, A::Error> {
            let mut metadata = Metadata::default();
            let mut seen = HashSet::new();
            while let Some(key) = map.next_key::<String>()? {
                self.budget.keys += 1;
                if self.budget.keys > MAX_KEYS || key.len() > 256 || !seen.insert(key.clone()) {
                    return Err(de::Error::custom(
                        "Bundle property list has excessive or duplicate keys",
                    ));
                }
                let value = map.next_value_seed(Seed {
                    budget: self.budget,
                    depth: self.depth + 1,
                })?;
                if self.depth == 0 {
                    let field = match key.as_str() {
                        "CFBundleDisplayName" => Some(&mut metadata.display_name),
                        "CFBundleName" => Some(&mut metadata.name),
                        "CFBundleIdentifier" => Some(&mut metadata.identifier),
                        "CFBundleShortVersionString" => Some(&mut metadata.version),
                        "CFBundleVersion" => Some(&mut metadata.build),
                        _ => None,
                    };
                    if let Some(field) = field {
                        *field = Some(value.text.ok_or_else(|| {
                            de::Error::custom("Bundle metadata field is not a string")
                        })?);
                    }
                }
            }
            Ok(Parsed {
                text: None,
                metadata: (self.depth == 0).then_some(metadata),
            })
        }
    }

    struct BoundedMetadata(Metadata);
    impl<'de> serde::Deserialize<'de> for BoundedMetadata {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let parsed = Seed {
                budget: &mut Budget::default(),
                depth: 0,
            }
            .deserialize(deserializer)?;
            parsed
                .metadata
                .map(Self)
                .ok_or_else(|| de::Error::custom("Bundle property list is not a dictionary"))
        }
    }

    fn parse(bytes: &[u8]) -> Result<Metadata, &'static str> {
        if bytes.len() > MAX_PLIST_BYTES {
            return Err("Bundle property list exceeds byte limit");
        }
        // Limit accepted encodings to documented macOS XML/binary metadata.
        let xml = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        if !bytes.starts_with(b"bplist00")
            && !xml
                .iter()
                .copied()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| b == b'<')
        {
            return Err("Bundle property list is not XML or binary");
        }
        let parsed: BoundedMetadata = plist::from_reader(Cursor::new(bytes))
            .map_err(|_| "Bundle property list is malformed or exceeds structural limits")?;
        // Serde represents plist dates as strings. After the bounded pass,
        // verify original value types before accepting observed string fields.
        let value = plist::Value::from_reader(Cursor::new(bytes))
            .map_err(|_| "Bundle property list is malformed")?;
        let dictionary = value
            .as_dictionary()
            .ok_or("Bundle property list is not a dictionary")?;
        for key in [
            "CFBundleDisplayName",
            "CFBundleName",
            "CFBundleIdentifier",
            "CFBundleShortVersionString",
            "CFBundleVersion",
        ] {
            if dictionary
                .get(key)
                .is_some_and(|value| value.as_string().is_none())
            {
                return Err("Bundle metadata field is not a string");
            }
        }
        Ok(parsed.0)
    }

    fn application(
        path: &Path,
        canonical: &Path,
        scope: InstallScope,
    ) -> Result<InstalledApp, &'static str> {
        let contents = path.join("Contents");
        let info = contents.join("Info.plist");
        let contents_metadata = fs::symlink_metadata(&contents)
            .map_err(|_| "Bundle contents are missing or unreadable")?;
        let info_metadata = fs::symlink_metadata(&info)
            .map_err(|_| "Bundle property list is missing or unreadable")?;
        if linked(&contents_metadata)
            || !contents_metadata.is_dir()
            || linked(&info_metadata)
            || !info_metadata.is_file()
        {
            return Err("Bundle contents or property list are linked or have an unexpected type");
        }
        if info_metadata.len() > MAX_PLIST_BYTES as u64 {
            return Err("Bundle property list exceeds byte limit");
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        }
        let file = options
            .open(&info)
            .map_err(|_| "Bundle property list could not be opened")?;
        let opened = file
            .metadata()
            .map_err(|_| "Bundle property list could not be inspected")?;
        if !opened.is_file() || opened.len() > MAX_PLIST_BYTES as u64 {
            return Err("Bundle property list has an unexpected type or exceeds byte limit");
        }
        let mut bytes = Vec::new();
        file.take((MAX_PLIST_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Bundle property list could not be read")?;
        let metadata = parse(&bytes)?;
        let name = text(metadata.display_name, 512)?
            .or(text(metadata.name, 512)?)
            .or(text(
                path.file_stem().and_then(|v| v.to_str()).map(str::to_owned),
                512,
            )?)
            .ok_or("Bundle has no readable application name")?;
        let native_id = text(metadata.identifier, 256)?;
        let version = text(metadata.version, 128)?.or(text(metadata.build, 128)?);
        let scope_tag = if scope == InstallScope::User {
            b"user:".as_slice()
        } else {
            b"machine:".as_slice()
        };
        let mut identity = b"kitlane:macos-bundle:".to_vec();
        identity.extend_from_slice(scope_tag);
        identity.extend_from_slice(canonical.as_os_str().as_encoded_bytes());
        Ok(InstalledApp {
            id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &identity).to_string(),
            name,
            publisher: None,
            version,
            scope,
            source: InventorySource::MacosBundle,
            native_id,
            origin: None,
        })
    }

    pub(super) fn scan_roots(
        device: DeviceInfo,
        roots: &[Root],
        initial_warnings: Vec<String>,
    ) -> Result<InventorySnapshot, InventoryError> {
        device.validate().map_err(InventoryError::InvalidDevice)?;
        let mut warnings = vec![COVERAGE.into()];
        for warning in initial_warnings {
            warn(&mut warnings, warning);
        }
        let mut apps = Vec::new();
        let mut visited = 0;
        let mut readable = false;
        let mut seen = HashSet::new();
        for root in roots {
            let metadata = match fs::symlink_metadata(&root.path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    warn(
                        &mut warnings,
                        format!(
                            "{} source {}",
                            root.label,
                            if error.kind() == std::io::ErrorKind::NotFound {
                                "is absent"
                            } else {
                                "could not be inspected"
                            }
                        ),
                    );
                    continue;
                }
            };
            if linked(&metadata) || !metadata.is_dir() || !unlinked(&root.path).unwrap_or(false) {
                warn(
                    &mut warnings,
                    format!(
                        "{} source is linked, unreadable, or not a directory",
                        root.label
                    ),
                );
                continue;
            }
            let canonical_root = match fs::canonicalize(&root.path) {
                Ok(path) => path,
                Err(_) => {
                    warn(
                        &mut warnings,
                        format!("{} source could not be resolved", root.label),
                    );
                    continue;
                }
            };
            let mut pending = vec![(root.path.clone(), 0)];
            while let Some((directory, depth)) = pending.pop() {
                let entries = match fs::read_dir(directory) {
                    Ok(entries) => {
                        if depth == 0 {
                            readable = true;
                        }
                        entries
                    }
                    Err(_) => {
                        if depth == 0 {
                            warn(
                                &mut warnings,
                                format!("{} source could not be read", root.label),
                            );
                        } else {
                            warn(
                                &mut warnings,
                                "Nested application directory could not be read",
                            );
                        }
                        continue;
                    }
                };
                for entry in entries {
                    visited += 1;
                    if visited > MAX_ENTRIES {
                        return Err(InventoryError::LimitExceeded);
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => {
                            warn(
                                &mut warnings,
                                "Application directory entry could not be read",
                            );
                            continue;
                        }
                    };
                    let path = entry.path();
                    let metadata = match fs::symlink_metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(_) => {
                            warn(
                                &mut warnings,
                                "Application directory entry could not be inspected",
                            );
                            continue;
                        }
                    };
                    if linked(&metadata) {
                        warn(&mut warnings, "Linked application directory entry skipped");
                        continue;
                    }
                    if !metadata.is_dir() {
                        continue;
                    }
                    let canonical = match fs::canonicalize(&path) {
                        Ok(path) if path.starts_with(&canonical_root) => path,
                        _ => {
                            warn(
                                &mut warnings,
                                "Application directory entry could not be safely resolved",
                            );
                            continue;
                        }
                    };
                    if path
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("app"))
                    {
                        let scope_tag = root.scope == InstallScope::User;
                        if !seen.insert((scope_tag, canonical.clone())) {
                            continue;
                        }
                        match application(&path, &canonical, root.scope) {
                            Ok(app) => {
                                if apps.len() == MAX_APPS {
                                    return Err(InventoryError::LimitExceeded);
                                }
                                if app.native_id.is_none() {
                                    warn(
                                        &mut warnings,
                                        "Application bundle has no observed bundle identifier",
                                    );
                                }
                                apps.push(app);
                            }
                            Err(message) => warn(&mut warnings, message),
                        }
                        continue; // Never inspect embedded apps in a bundle.
                    }
                    if path
                        .file_name()
                        .is_some_and(|v| v.as_encoded_bytes().starts_with(b"."))
                        || path.extension().is_some_and(|v| {
                            ["framework", "bundle", "plugin"]
                                .iter()
                                .any(|ext| v.eq_ignore_ascii_case(ext))
                        })
                    {
                        continue;
                    }
                    if depth == MAX_DIRECTORY_DEPTH {
                        warn(
                            &mut warnings,
                            "Nested application directory skipped at depth limit",
                        );
                        continue;
                    }
                    pending.push((path, depth + 1));
                }
            }
        }
        if !readable {
            return Err(InventoryError::SourceUnavailable(
                "No macOS application source could be read",
            ));
        }
        apps.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        let scanned_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| InventoryError::InvalidClock)?
            .as_secs();
        if scanned_at_unix_secs == 0 {
            return Err(InventoryError::InvalidClock);
        }
        let snapshot = InventorySnapshot {
            device,
            scanned_at_unix_secs,
            apps,
            warnings,
        };
        snapshot
            .validate()
            .map_err(InventoryError::InvalidSnapshot)?;
        Ok(snapshot)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir()
                    .canonicalize()
                    .unwrap()
                    .join(format!("kitlane-macos-{}", uuid::Uuid::new_v4()));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn bundle(&self, name: &str, value: &plist::Value, binary: bool) -> PathBuf {
                let path = self.0.join(name);
                fs::create_dir_all(path.join("Contents")).unwrap();
                let info = path.join("Contents/Info.plist");
                if binary {
                    value.to_file_binary(info).unwrap();
                } else {
                    value.to_file_xml(info).unwrap();
                }
                path
            }
            fn scan(&self) -> InventorySnapshot {
                scan_roots(
                    device(),
                    &[Root::new(
                        &self.0,
                        InstallScope::Machine,
                        "fixture applications",
                    )],
                    Vec::new(),
                )
                .unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fn device() -> DeviceInfo {
            DeviceInfo {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Fixture device".into(),
                platform: crate::model::Platform::Macos,
                architecture: "arm64".into(),
            }
        }
        fn metadata() -> plist::Value {
            let mut values = plist::Dictionary::new();
            for (key, value) in [
                ("CFBundleDisplayName", "Display name"),
                ("CFBundleName", "Short name"),
                ("CFBundleIdentifier", "org.example.app"),
                ("CFBundleShortVersionString", "2.0"),
                ("CFBundleVersion", "200"),
            ] {
                values.insert(key.into(), plist::Value::String(value.into()));
            }
            plist::Value::Dictionary(values)
        }
        #[test]
        fn xml_and_binary_metadata_are_observations_with_opaque_ids() {
            let fixture = Fixture::new();
            fixture.bundle("Xml.app", &metadata(), false);
            fixture.bundle("Binary.app", &metadata(), true);
            let first = fixture.scan();
            assert_eq!(first.apps.len(), 2);
            for app in &first.apps {
                assert_eq!(app.name, "Display name");
                assert_eq!(app.version.as_deref(), Some("2.0"));
                assert_eq!(app.native_id.as_deref(), Some("org.example.app"));
                assert_eq!(app.publisher, None);
                assert_eq!(app.origin, None);
                assert_eq!(app.source, InventorySource::MacosBundle);
                assert!(uuid::Uuid::parse_str(&app.id).is_ok());
            }
            assert_ne!(first.apps[0].id, first.apps[1].id); // Bundle ID alone never merges copies.
            assert_eq!(first.apps, fixture.scan().apps);
            let serialized = serde_json::to_string(&first).unwrap();
            assert!(!serialized.contains(&fixture.0.to_string_lossy().replace('\\', "\\\\")));
        }
        #[test]
        fn missing_metadata_uses_filename_and_build_version_without_inventing_publisher() {
            let fixture = Fixture::new();
            let mut dictionary = plist::Dictionary::new();
            dictionary.insert("CFBundleVersion".into(), plist::Value::String("7".into()));
            fixture.bundle("Unknown.app", &plist::Value::Dictionary(dictionary), false);
            let snapshot = fixture.scan();
            assert_eq!(snapshot.apps[0].name, "Unknown");
            assert_eq!(snapshot.apps[0].version.as_deref(), Some("7"));
            assert_eq!(snapshot.apps[0].native_id, None);
            assert_eq!(snapshot.apps[0].publisher, None);
            assert!(
                snapshot
                    .warnings
                    .iter()
                    .any(|v| v.contains("no observed bundle identifier"))
            );
        }
        #[test]
        fn rejects_malformed_oversized_and_wrong_typed_metadata() {
            let fixture = Fixture::new();
            let malformed = fixture.bundle("Malformed.app", &metadata(), false);
            fs::write(malformed.join("Contents/Info.plist"), b"<plist><dict>").unwrap();
            let oversized = fixture.bundle("Oversized.app", &metadata(), false);
            fs::write(
                oversized.join("Contents/Info.plist"),
                vec![b' '; MAX_PLIST_BYTES + 1],
            )
            .unwrap();
            let mut wrong = plist::Dictionary::new();
            wrong.insert("CFBundleDisplayName".into(), plist::Value::Boolean(true));
            fixture.bundle("Wrong.app", &plist::Value::Dictionary(wrong), true);
            let mut controls = plist::Dictionary::new();
            controls.insert(
                "CFBundleName".into(),
                plist::Value::String("Bad\nname".into()),
            );
            fixture.bundle("Controls.app", &plist::Value::Dictionary(controls), false);
            let snapshot = fixture.scan();
            assert!(snapshot.apps.is_empty());
            assert_eq!(snapshot.warnings.len(), 5);
        }
        #[test]
        fn validates_original_string_types_duplicate_keys_and_xml_bom() {
            let mut dictionary = plist::Dictionary::new();
            dictionary.insert(
                "CFBundleName".into(),
                plist::Value::Date(plist::Date::from_xml_format("2026-10-11T00:00:00Z").unwrap()),
            );
            let mut bytes = Vec::new();
            plist::Value::Dictionary(dictionary)
                .to_writer_binary(&mut bytes)
                .unwrap();
            assert!(parse(&bytes).is_err());
            assert!(parse(b"<plist><dict><key>CFBundleName</key><string>First</string><key>CFBundleName</key><string>Second</string></dict></plist>").is_err());
            bytes = vec![0xef, 0xbb, 0xbf];
            metadata().to_writer_xml(&mut bytes).unwrap();
            assert!(parse(&bytes).is_ok());
        }

        #[test]
        fn bounds_keys_depth_and_text_before_building_a_value_tree() {
            let mut keys = plist::Dictionary::new();
            for i in 0..=MAX_KEYS {
                keys.insert(format!("Key{i}"), plist::Value::String("value".into()));
            }
            let mut bytes = Vec::new();
            plist::Value::Dictionary(keys)
                .to_writer_binary(&mut bytes)
                .unwrap();
            assert!(parse(&bytes).is_err());
            let mut deep = plist::Value::String("value".into());
            for _ in 0..=MAX_PLIST_DEPTH {
                deep = plist::Value::Array(vec![deep]);
            }
            bytes.clear();
            deep.to_writer_binary(&mut bytes).unwrap();
            assert!(parse(&bytes).is_err());
            let mut dictionary = plist::Dictionary::new();
            dictionary.insert("CFBundleName".into(), plist::Value::String("x".repeat(513)));
            let fixture = Fixture::new();
            fixture.bundle("Long.app", &plist::Value::Dictionary(dictionary), false);
            assert!(fixture.scan().apps.is_empty());
        }
        #[test]
        fn traversal_is_bounded_deduplicates_paths_and_preserves_scopes() {
            let fixture = Fixture::new();
            fixture.bundle("Top.app", &metadata(), false);
            fixture.bundle("Utilities/Nested.app", &metadata(), false);
            fixture.bundle("A/B/AtLimit.app", &metadata(), false);
            fixture.bundle("A/B/C/TooDeep.app", &metadata(), false);
            fixture.bundle("Top.app/Contents/Embedded.app", &metadata(), false);
            let roots = [
                Root::new(&fixture.0, InstallScope::Machine, "fixture applications"),
                Root::new(&fixture.0, InstallScope::Machine, "fixture applications"),
            ];
            let snapshot = scan_roots(device(), &roots, Vec::new()).unwrap();
            assert_eq!(snapshot.apps.len(), 3);
            assert!(snapshot.warnings.iter().any(|v| v.contains("depth limit")));
            let roots = [
                Root::new(&fixture.0, InstallScope::Machine, "fixture applications"),
                Root::new(&fixture.0, InstallScope::User, "fixture applications"),
            ];
            let scoped = scan_roots(device(), &roots, Vec::new()).unwrap();
            assert_eq!(scoped.apps.len(), 6);
            assert_eq!(
                scoped
                    .apps
                    .iter()
                    .map(|a| &a.id)
                    .collect::<HashSet<_>>()
                    .len(),
                6
            );
        }
        #[test]
        fn stable_ids_survive_metadata_changes_and_warnings_do_not_expose_paths() {
            let fixture = Fixture::new();
            let path = fixture.bundle("Stable.app", &metadata(), false);
            let first = fixture.scan().apps.remove(0).id;
            let mut changed = metadata();
            changed.as_dictionary_mut().unwrap().insert(
                "CFBundleShortVersionString".into(),
                plist::Value::String("3.0".into()),
            );
            changed
                .to_file_binary(path.join("Contents/Info.plist"))
                .unwrap();
            assert_eq!(first, fixture.scan().apps[0].id);
            let snapshot = scan_roots(
                device(),
                &[
                    Root::new(&fixture.0, InstallScope::Machine, "fixture applications"),
                    Root::new(
                        fixture.0.join("PrivateMissing"),
                        InstallScope::User,
                        "current-user applications",
                    ),
                ],
                Vec::new(),
            )
            .unwrap();
            assert!(snapshot.warnings.iter().any(|v| v.contains("is absent")));
            assert!(
                snapshot
                    .warnings
                    .iter()
                    .all(|v| !v.contains("PrivateMissing") && !v.contains("Stable.app"))
            );
        }
        #[test]
        fn absent_and_non_directory_sources_are_explicit_errors() {
            let fixture = Fixture::new();
            let file = fixture.0.join("source-file");
            fs::write(&file, b"no directory").unwrap();
            for path in [fixture.0.join("absent"), file] {
                assert!(matches!(
                    scan_roots(
                        device(),
                        &[Root::new(
                            path,
                            InstallScope::Machine,
                            "fixture applications"
                        )],
                        Vec::new()
                    ),
                    Err(InventoryError::SourceUnavailable(_))
                ));
            }
            assert!(fixture.scan().apps.is_empty()); // Readable empty source is distinct.
        }
        #[test]
        fn warning_and_value_counts_are_bounded() {
            let mut warnings = Vec::new();
            for _ in 0..100 {
                warn(&mut warnings, "Bundle metadata warning");
            }
            assert_eq!(warnings.len(), 64);
            assert!(warnings[63].contains("suppressed"));
            let mut dictionary = plist::Dictionary::new();
            dictionary.insert(
                "Many".into(),
                plist::Value::Array(vec![plist::Value::Boolean(true); MAX_VALUES]),
            );
            let mut bytes = Vec::new();
            plist::Value::Dictionary(dictionary)
                .to_writer_binary(&mut bytes)
                .unwrap();
            assert!(parse(&bytes).is_err());
        }
        #[cfg(unix)]
        #[test]
        fn does_not_follow_bundle_directory_metadata_or_source_links() {
            use std::os::unix::fs::symlink;
            let fixture = Fixture::new();
            let outside = Fixture::new();
            let bundle = outside.bundle("External.app", &metadata(), false);
            symlink(&bundle, fixture.0.join("Linked.app")).unwrap();
            symlink(&outside.0, fixture.0.join("LinkedDirectory")).unwrap();
            let linked_info = fixture.bundle("InfoLink.app", &metadata(), false);
            fs::remove_file(linked_info.join("Contents/Info.plist")).unwrap();
            symlink(
                bundle.join("Contents/Info.plist"),
                linked_info.join("Contents/Info.plist"),
            )
            .unwrap();
            let linked_contents = fixture.0.join("ContentsLink.app");
            fs::create_dir(&linked_contents).unwrap();
            symlink(bundle.join("Contents"), linked_contents.join("Contents")).unwrap();
            assert!(fixture.scan().apps.is_empty());
            let root = Root::new(
                fixture.0.join("LinkedDirectory"),
                InstallScope::Machine,
                "fixture applications",
            );
            assert!(matches!(
                scan_roots(device(), &[root], Vec::new()),
                Err(InventoryError::SourceUnavailable(_))
            ));
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
#[test]
fn native_scan_rejects_non_macos_hosts() {
    let device = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Fixture device".into(),
        platform: crate::model::Platform::Macos,
        architecture: "arm64".into(),
    };
    assert_eq!(scan(device), Err(InventoryError::UnsupportedPlatform));
}

#[cfg(all(test, target_os = "macos"))]
#[test]
#[ignore = "Read-only local macOS integration check; requires a Mac"]
fn reads_standard_macos_sources_without_launching_applications() {
    let device = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Local macOS validation".into(),
        platform: crate::model::Platform::Macos,
        architecture: std::env::consts::ARCH.into(),
    };
    let snapshot = scan(device).unwrap();
    snapshot.validate().unwrap();
    println!(
        "Observed {} application bundles; {} coverage warnings",
        snapshot.apps.len(),
        snapshot.warnings.len()
    );
}
