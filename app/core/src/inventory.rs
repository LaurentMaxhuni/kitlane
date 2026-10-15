//! Read-only platform inventory dispatch. Metadata is observation, not
//! verified product identity or proof that an installer/update is available.
use crate::model::{DeviceInfo, InventorySnapshot};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryError {
    UnsupportedPlatform,
    InvalidDevice(&'static str),
    RegistryUnavailable,
    LimitExceeded,
    InvalidClock,
    InvalidSnapshot(&'static str),
    SourceUnavailable(&'static str),
    ScanFailed(String),
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedPlatform => {
                "Installed application scanning is unavailable on this platform"
            }
            Self::InvalidDevice(message) | Self::InvalidSnapshot(message) => message,
            Self::SourceUnavailable(message) => message,
            Self::ScanFailed(message) => message,
            Self::RegistryUnavailable => "No Windows uninstall registry source could be read",
            Self::LimitExceeded => "Registry inventory exceeded its scan or application limit",
            Self::InvalidClock => "System clock cannot provide a valid inventory timestamp",
        })
    }
}

impl std::error::Error for InventoryError {}

/// Scan platform application observations. Never installs or updates programs.
/// Partial access failures produce warnings; complete access failure is an error.
pub fn scan(device: DeviceInfo) -> Result<InventorySnapshot, InventoryError> {
    device.validate().map_err(InventoryError::InvalidDevice)?;
    #[cfg(windows)]
    {
        if device.platform != crate::model::Platform::Windows {
            return Err(InventoryError::InvalidDevice(
                "Device platform does not match Windows scanner",
            ));
        }
        windows::scan(device)
    }
    #[cfg(target_os = "macos")]
    {
        crate::inventory_macos::scan(device)
    }
    #[cfg(target_os = "linux")]
    {
        crate::inventory_linux::scan(device)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err(InventoryError::UnsupportedPlatform)
    }
}

/// Local-only evidence. Never included in a stored or shared snapshot.
pub struct LocalIconSource {
    pub app_id: String,
    pub app_name: String,
    pub display_icon: Option<String>,
    pub install_location: Option<String>,
    pub uninstall: Option<String>,
}

pub struct LocalInventory {
    pub snapshot: InventorySnapshot,
    pub icons: Vec<LocalIconSource>,
}

pub fn scan_with_icons(device: DeviceInfo) -> Result<LocalInventory, InventoryError> {
    #[cfg(windows)]
    {
        device.validate().map_err(InventoryError::InvalidDevice)?;
        if device.platform != crate::model::Platform::Windows {
            return Err(InventoryError::UnsupportedPlatform);
        }
        windows::scan_with_icons(device)
    }
    #[cfg(not(windows))]
    {
        Ok(LocalInventory {
            snapshot: scan(device)?,
            icons: Vec::new(),
        })
    }
}

#[cfg(any(windows, test))]
mod records {
    use super::*;
    use crate::model::{InstallScope, InstalledApp, InventorySource};
    use std::collections::HashMap;

    pub(super) const MAX_APPS: usize = 10_000;
    #[cfg(windows)]
    pub(super) const MAX_KEYS: usize = 40_000;
    pub(super) const COVERAGE: &str = "Coverage: current-user and machine uninstall registry only. MSIX/Store apps, portable apps, other users, and unregistered apps are not scanned; registry entries may be stale.";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum View {
        X64,
        X32,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Entry {
        pub key: String,
        pub view: View,
        pub scope: InstallScope,
        pub name: String,
        pub publisher: Option<String>,
        pub version: Option<String>,
        // Private evidence: never emitted in the snapshot or warnings.
        pub location: Option<String>,
        pub uninstall: Option<String>,
        pub windows_installer: bool,
        pub icon: Option<String>,
    }

    #[derive(Default)]
    pub(super) struct Metadata {
        pub name: Option<String>,
        pub publisher: Option<String>,
        pub version: Option<String>,
        pub location: Option<String>,
        pub uninstall: Option<String>,
        pub system_component: Option<u32>,
        pub parent_key: Option<String>,
        pub parent_name: Option<String>,
        pub release_type: Option<String>,
        pub windows_installer: Option<u32>,
        pub icon: Option<String>,
    }

    pub(super) fn text(value: Option<String>, max: usize) -> Result<Option<String>, &'static str> {
        let Some(value) = value else {
            return Ok(None);
        };
        // Check controls before trimming: do not conceal malformed registry data.
        if value.chars().any(char::is_control) {
            return Err("Registry entry contains control characters");
        }
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        if value.len() > max {
            return Err("Registry entry contains oversized text");
        }
        Ok(Some(value.to_owned()))
    }

    pub(super) fn entry(
        key: String,
        view: View,
        scope: InstallScope,
        raw: Metadata,
    ) -> Result<Option<Entry>, &'static str> {
        if key.len() > 1024 || key.trim().is_empty() || key.chars().any(char::is_control) {
            return Err("Registry entry has an invalid key");
        }
        let parent_key = text(raw.parent_key, 1024)?;
        let parent_name = text(raw.parent_name, 512)?;
        let release_type = text(raw.release_type, 128)?;
        let update = release_type.as_deref().is_some_and(|v| {
            [
                "update",
                "security update",
                "update rollup",
                "hotfix",
                "service pack",
            ]
            .iter()
            .any(|kind| v.eq_ignore_ascii_case(kind))
        });
        // No name heuristics: runtimes and apps named "Update" can be legitimate.
        if raw.system_component == Some(1)
            || parent_key.is_some()
            || parent_name.is_some()
            || update
        {
            return Ok(None);
        }
        let Some(name) = text(raw.name, 512)? else {
            return Ok(None);
        };
        Ok(Some(Entry {
            key,
            view,
            scope,
            name,
            publisher: text(raw.publisher, 512)?,
            version: text(raw.version, 128)?,
            location: text(raw.location, 4096)?,
            uninstall: text(raw.uninstall, 4096)?,
            windows_installer: raw.windows_installer == Some(1),
            icon: text(raw.icon, 4096).ok().flatten(),
        }))
    }

    fn same_install(a: &Entry, b: &Entry) -> bool {
        a.view != b.view
            && a.scope == b.scope
            && a.key.eq_ignore_ascii_case(&b.key)
            && a.name == b.name
            && a.publisher == b.publisher
            && a.version == b.version
            && a.location == b.location
            && a.uninstall == b.uninstall
            && a.windows_installer == b.windows_installer
            && (a.uninstall.is_some()
                || (a.windows_installer && uuid::Uuid::parse_str(&a.key).is_ok()))
    }

    #[cfg(test)]
    pub(super) fn applications(entries: Vec<Entry>) -> Result<Vec<InstalledApp>, InventoryError> {
        applications_with_icons(entries, &mut Vec::new())
    }

    pub(super) fn applications_with_icons(
        entries: Vec<Entry>,
        icons: &mut Vec<LocalIconSource>,
    ) -> Result<Vec<InstalledApp>, InventoryError> {
        let mut groups: HashMap<(u8, String), Vec<Entry>> = HashMap::new();
        for observation in entries {
            let scope = if observation.scope == InstallScope::User {
                0
            } else {
                1
            };
            let group = groups
                .entry((scope, observation.key.to_lowercase()))
                .or_default();
            if let Some(existing) = group.iter_mut().find(|v| same_install(v, &observation)) {
                // Prefer 64-bit observation, independent of enumeration order.
                if observation.view == View::X64 {
                    *existing = observation;
                }
            } else {
                group.push(observation);
            }
        }
        let count: usize = groups.values().map(Vec::len).sum();
        if count > MAX_APPS {
            return Err(InventoryError::LimitExceeded);
        }
        let mut apps = Vec::with_capacity(count);
        for ((scope, key), group) in groups {
            for item in group {
                let view = if item.view == View::X64 { 64 } else { 32 };
                let identity = format!("kitlane:windows-uninstall:{scope}:{view}:{key}");
                let id =
                    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, identity.as_bytes()).to_string();
                let native_id = crate::catalog::windows_native_id(&item.key);
                if item.icon.is_some() || item.location.is_some() || item.uninstall.is_some() {
                    icons.push(LocalIconSource {
                        app_id: id.clone(),
                        app_name: item.name.clone(),
                        display_icon: item.icon,
                        install_location: item.location,
                        uninstall: item.uninstall,
                    });
                }
                apps.push(InstalledApp {
                    id,
                    name: item.name,
                    publisher: item.publisher,
                    version: item.version,
                    scope: item.scope,
                    source: InventorySource::WindowsRegistry,
                    native_id,
                    origin: None,
                });
            }
        }
        apps.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        Ok(apps)
    }

    pub(super) fn warn(warnings: &mut Vec<String>, message: String) {
        if warnings.contains(&message) {
            return;
        }
        if warnings.len() < 63 {
            warnings.push(message);
        } else if warnings.len() == 63 {
            warnings.push("Additional inventory warnings were suppressed.".into());
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn fixture(key: &str, view: View, scope: InstallScope) -> Entry {
            entry(
                key.into(),
                view,
                scope,
                Metadata {
                    name: Some("Example App".into()),
                    publisher: Some("Example Ltd".into()),
                    version: Some("1.0".into()),
                    location: Some("C:\\Example".into()),
                    uninstall: Some("C:\\Example\\uninstall.exe".into()),
                    ..Metadata::default()
                },
            )
            .unwrap()
            .unwrap()
        }

        #[test]
        fn explicit_hidden_updates_and_children_are_filtered() {
            for raw in [
                Metadata {
                    system_component: Some(1),
                    ..Metadata::default()
                },
                Metadata {
                    parent_key: Some("Parent".into()),
                    ..Metadata::default()
                },
                Metadata {
                    parent_name: Some("Parent".into()),
                    ..Metadata::default()
                },
                Metadata {
                    release_type: Some("Security Update".into()),
                    ..Metadata::default()
                },
            ] {
                assert!(
                    entry("Key".into(), View::X64, InstallScope::User, raw)
                        .unwrap()
                        .is_none()
                );
            }
            let legitimate = Metadata {
                name: Some("Update Helper Runtime".into()),
                ..Metadata::default()
            };
            assert!(
                entry("Key".into(), View::X64, InstallScope::User, legitimate)
                    .unwrap()
                    .is_some()
            );
        }

        #[test]
        fn icon_locations_stay_local_and_bad_icon_metadata_does_not_remove_apps() {
            let observation = entry(
                "Example".into(),
                View::X64,
                InstallScope::User,
                Metadata {
                    name: Some("Example".into()),
                    icon: Some("bad\nicon".into()),
                    ..Metadata::default()
                },
            )
            .unwrap()
            .unwrap();
            let mut icons = Vec::new();
            let apps = applications_with_icons(vec![observation], &mut icons).unwrap();
            assert_eq!(apps.len(), 1);
            assert!(icons.is_empty());
            let observation = entry(
                "Example".into(),
                View::X64,
                InstallScope::User,
                Metadata {
                    name: Some("Example".into()),
                    icon: Some(r"C:\Apps\Example.exe,0".into()),
                    ..Metadata::default()
                },
            )
            .unwrap()
            .unwrap();
            let apps = applications_with_icons(vec![observation], &mut icons).unwrap();
            assert_eq!(icons[0].app_id, apps[0].id);
            assert!(
                !serde_json::to_string(&apps)
                    .unwrap()
                    .contains("Example.exe")
            );
        }

        #[test]
        fn malformed_and_absent_metadata_never_produce_invalid_apps() {
            for name in ["Bad\nName".into(), "x".repeat(513)] {
                assert!(
                    entry(
                        "Key".into(),
                        View::X64,
                        InstallScope::User,
                        Metadata {
                            name: Some(name),
                            ..Metadata::default()
                        }
                    )
                    .is_err()
                );
            }
            assert!(
                entry(
                    "Key".into(),
                    View::X64,
                    InstallScope::User,
                    Metadata::default()
                )
                .unwrap()
                .is_none()
            );
            let trimmed = entry(
                "Key".into(),
                View::X64,
                InstallScope::User,
                Metadata {
                    name: Some("  Example  ".into()),
                    publisher: Some("  ".into()),
                    ..Metadata::default()
                },
            )
            .unwrap()
            .unwrap();
            assert_eq!(trimmed.name, "Example");
            assert_eq!(trimmed.publisher, None);
            assert!(text(Some("publisher\0".into()), 512).is_err());
            assert!(text(Some("é".repeat(257)), 512).is_err());
        }

        #[test]
        fn merges_only_matching_cross_view_install_evidence() {
            let original = fixture("Example", View::X64, InstallScope::User);
            let mut mirror = original.clone();
            mirror.view = View::X32;
            assert_eq!(
                applications(vec![mirror.clone(), original.clone()])
                    .unwrap()
                    .len(),
                1
            );
            mirror.scope = InstallScope::Machine;
            assert_eq!(
                applications(vec![original.clone(), mirror.clone()])
                    .unwrap()
                    .len(),
                2
            );
            mirror.scope = InstallScope::User;
            mirror.location = Some("C:\\Other".into());
            assert_eq!(
                applications(vec![original.clone(), mirror.clone()])
                    .unwrap()
                    .len(),
                2
            );
            mirror.location = original.location.clone();
            mirror.key = "OtherRegistration".into();
            assert_eq!(
                applications(vec![original.clone(), mirror.clone()])
                    .unwrap()
                    .len(),
                2
            );
            mirror.key = original.key.clone();
            mirror.version = Some("2.0".into());
            assert_eq!(
                applications(vec![original.clone(), mirror]).unwrap().len(),
                2
            );
            let mut msi = original.clone();
            msi.key = "{7283BD23-7599-4444-9739-5A64F204152B}".into();
            msi.location = None;
            msi.uninstall = None;
            msi.windows_installer = true;
            let mut msi_mirror = msi.clone();
            msi_mirror.view = View::X32;
            assert_eq!(applications(vec![msi, msi_mirror]).unwrap().len(), 1);
            let separate_key = fixture(" Example ", View::X64, InstallScope::User);
            assert_eq!(
                applications(vec![original.clone(), separate_key])
                    .unwrap()
                    .len(),
                2
            );
            let mut no_evidence = original.clone();
            no_evidence.uninstall = None;
            let mut other_view = no_evidence.clone();
            other_view.view = View::X32;
            assert_eq!(
                applications(vec![no_evidence, other_view]).unwrap().len(),
                2
            );
        }

        #[test]
        fn native_identifiers_expose_only_catalog_recognized_registration_keys() {
            let recognized =
                applications(vec![fixture("7-Zip", View::X64, InstallScope::User)]).unwrap();
            assert_eq!(recognized[0].native_id.as_deref(), Some("7-Zip"));
            let private =
                applications(vec![fixture("Private-key", View::X64, InstallScope::User)]).unwrap();
            assert_eq!(private[0].native_id, None);
        }

        #[test]
        fn ids_are_opaque_stable_across_upgrades_and_scan_order() {
            let original = fixture("Private-key", View::X64, InstallScope::User);
            let mut upgraded = original.clone();
            upgraded.version = Some("2.0".into());
            let first = applications(vec![original.clone()]).unwrap();
            assert_eq!(first[0].id, applications(vec![upgraded]).unwrap()[0].id);
            assert!(uuid::Uuid::parse_str(&first[0].id).is_ok());
            let other = fixture("Other-key", View::X32, InstallScope::Machine);
            assert_eq!(
                applications(vec![original.clone(), other.clone()]).unwrap(),
                applications(vec![other, original]).unwrap()
            );
        }

        #[test]
        fn warnings_and_record_counts_are_bounded() {
            let mut warnings = vec![COVERAGE.into()];
            for i in 0..100 {
                warn(&mut warnings, format!("Warning {i}"));
            }
            assert_eq!(warnings.len(), 64);
            assert!(warnings.last().unwrap().contains("suppressed"));
            let entries = (0..=MAX_APPS)
                .map(|i| fixture(&format!("Key{i}"), View::X64, InstallScope::User))
                .collect();
            assert_eq!(applications(entries), Err(InventoryError::LimitExceeded));
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{records::*, *};
    use crate::model::InstallScope;
    use std::{
        io,
        time::{SystemTime, UNIX_EPOCH},
    };
    use winreg::{RegKey, enums::*};

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const MAX_VALUE_BYTES: usize = 16_384;

    fn string_value(key: &RegKey, name: &str) -> Result<Option<String>, &'static str> {
        let value = match key.get_raw_value(name) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Registry metadata could not be read"),
        };
        if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
            return Err("Registry string metadata has an unexpected value type");
        }
        decode_string(&value.bytes).map(Some)
    }

    fn decode_string(bytes: &[u8]) -> Result<String, &'static str> {
        if bytes.len() > MAX_VALUE_BYTES || !bytes.len().is_multiple_of(2) {
            return Err("Registry string metadata has an invalid size");
        }
        let mut words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        while words.last() == Some(&0) {
            words.pop();
        }
        String::from_utf16(&words).map_err(|_| "Registry string metadata contains invalid UTF-16")
    }

    fn dword_value(key: &RegKey, name: &str) -> Result<Option<u32>, &'static str> {
        match key.get_raw_value(name) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("Registry metadata could not be read"),
            Ok(value) if value.vtype == REG_DWORD && value.bytes.len() == 4 => Ok(Some(
                u32::from_le_bytes(value.bytes.as_ref().try_into().unwrap()),
            )),
            Ok(_) => Err("Registry flag metadata has an unexpected value type or size"),
        }
    }

    fn metadata(key: &RegKey) -> Result<Metadata, &'static str> {
        // Avoid reading oversized installer metadata. A concurrent registry writer
        // can still change a value between this query and winreg's allocation.
        let info = key
            .query_info()
            .map_err(|_| "Registry entry metadata could not be queried")?;
        if info.max_value_len as usize > MAX_VALUE_BYTES {
            return Err("Registry entry contains oversized value data");
        }
        Ok(Metadata {
            name: string_value(key, "DisplayName")?,
            publisher: string_value(key, "Publisher")?,
            version: string_value(key, "DisplayVersion")?,
            location: string_value(key, "InstallLocation")?,
            uninstall: string_value(key, "UninstallString")?,
            system_component: dword_value(key, "SystemComponent")?,
            parent_key: string_value(key, "ParentKeyName")?,
            parent_name: string_value(key, "ParentDisplayName")?,
            release_type: string_value(key, "ReleaseType")?,
            windows_installer: dword_value(key, "WindowsInstaller")?,
            icon: string_value(key, "DisplayIcon").ok().flatten(),
        })
    }

    pub(super) fn scan(device: DeviceInfo) -> Result<InventorySnapshot, InventoryError> {
        scan_with_icons(device).map(|local| local.snapshot)
    }

    pub(super) fn scan_with_icons(device: DeviceInfo) -> Result<LocalInventory, InventoryError> {
        let mut warnings = vec![COVERAGE.into()];
        let mut entries = Vec::new();
        let mut visited = 0;
        let mut opened = 0;
        let mut failed = false;
        for (hive, scope, label) in [
            (HKEY_CURRENT_USER, InstallScope::User, "HKCU"),
            (HKEY_LOCAL_MACHINE, InstallScope::Machine, "HKLM"),
        ] {
            for (view, flag, bits) in [
                (View::X64, KEY_WOW64_64KEY, 64),
                (View::X32, KEY_WOW64_32KEY, 32),
            ] {
                let root = match RegKey::predef(hive)
                    .open_subkey_with_flags(UNINSTALL, KEY_READ | flag)
                {
                    Ok(key) => {
                        opened += 1;
                        key
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        warn(
                            &mut warnings,
                            format!("{label} {bits}-bit uninstall root is absent."),
                        );
                        continue;
                    }
                    Err(error) => {
                        failed = true;
                        warn(
                            &mut warnings,
                            format!(
                                "{label} {bits}-bit uninstall root could not be read (OS error {:?}).",
                                error.raw_os_error()
                            ),
                        );
                        continue;
                    }
                };
                for key_name in root.enum_keys() {
                    visited += 1;
                    if visited > MAX_KEYS {
                        return Err(InventoryError::LimitExceeded);
                    }
                    let result = key_name
                        .map_err(|_| "Registry subkey enumeration failed")
                        .and_then(|name| {
                            let child = root
                                .open_subkey_with_flags(&name, KEY_READ | flag)
                                .map_err(|_| "Registry entry could not be opened")?;
                            entry(name, view, scope, metadata(&child)?)
                        });
                    match result {
                        Ok(Some(app)) => entries.push(app),
                        Ok(None) => {}
                        Err(message) => warn(
                            &mut warnings,
                            format!("{label} {bits}-bit: {message}; an entry was skipped."),
                        ),
                    }
                }
            }
        }
        if opened == 0 && failed {
            return Err(InventoryError::RegistryUnavailable);
        }
        let scanned_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| InventoryError::InvalidClock)?
            .as_secs();
        if scanned_at_unix_secs == 0 {
            return Err(InventoryError::InvalidClock);
        }
        let mut icons = Vec::new();
        let snapshot = InventorySnapshot {
            device,
            scanned_at_unix_secs,
            apps: applications_with_icons(entries, &mut icons)?,
            warnings,
        };
        snapshot
            .validate()
            .map_err(InventoryError::InvalidSnapshot)?;
        Ok(LocalInventory { snapshot, icons })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::model::Platform;

        #[test]
        fn strict_utf16_rejects_malformed_registry_data() {
            assert!(decode_string(&[0x00]).is_err());
            assert!(decode_string(&[0x00, 0xd8]).is_err());
            assert!(decode_string(&vec![0; MAX_VALUE_BYTES + 2]).is_err());
            assert_eq!(decode_string(&[b'A', 0, 0, 0]).unwrap(), "A");
            assert!(text(Some(decode_string(&[b'A', 0, 0, 0, b'B', 0]).unwrap()), 512).is_err());
        }

        #[test]
        #[ignore = "read-only local registry integration; run explicitly on Windows"]
        fn reads_current_user_and_machine_registry_without_writes() {
            let device = DeviceInfo {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Inventory test".into(),
                platform: Platform::Windows,
                architecture: std::env::consts::ARCH.into(),
            };
            let snapshot = super::super::scan(device).expect("local registry inventory");
            assert!(snapshot.scanned_at_unix_secs > 0);
            assert!(snapshot.warnings.iter().any(|v| v == COVERAGE));
            snapshot.validate().unwrap();
            println!(
                "Read-only inventory: {} applications, {} warnings",
                snapshot.apps.len(),
                snapshot.warnings.len()
            );
        }
    }
}

#[cfg(all(test, not(any(windows, target_os = "macos", target_os = "linux"))))]
mod unsupported_tests {
    use super::*;
    use crate::model::Platform;

    #[test]
    fn unsupported_platform_is_explicit() {
        let device = DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            platform: Platform::current().unwrap_or(Platform::Linux),
            architecture: "x86_64".into(),
        };
        assert_eq!(scan(device), Err(InventoryError::UnsupportedPlatform));
    }
}
