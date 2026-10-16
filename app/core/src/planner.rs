//! Deterministic metadata comparison for human review. Never executes installs.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::{
    catalog::{Availability, Catalog, MatchOutcome},
    model::{DeviceInfo, InstalledApp, InventorySnapshot, InventorySource, Platform},
};

const MAX_PEERS: usize = 64;
const MAX_INPUT_APPS: usize = 50_000;
const MAX_CANDIDATES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    AlreadyPresent,
    ReviewRequired,
    Unavailable,
    Unknown,
}

impl PlanStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::AlreadyPresent => "Already present",
            Self::ReviewRequired => "Review required",
            Self::Unavailable => "Unavailable",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanEntry {
    pub product_id: Option<String>,
    pub name: String,
    pub status: PlanStatus,
    pub reason: String,
    /// Sorted, unique peer device UUIDs; callers resolve display names separately.
    pub source_devices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError(pub String);

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Cannot build compatibility plan: {}", self.0)
    }
}

impl std::error::Error for PlanError {}

fn observation(
    catalog: &Catalog,
    snapshot: &InventorySnapshot,
    app: &InstalledApp,
) -> MatchOutcome {
    let source_platform = match app.source {
        InventorySource::WindowsRegistry => Some(Platform::Windows),
        InventorySource::MacosBundle => Some(Platform::Macos),
        InventorySource::Flatpak => Some(Platform::Linux),
        InventorySource::Manual => None,
    };
    if source_platform.is_some_and(|platform| platform != snapshot.device.platform) {
        return MatchOutcome::Unknown(crate::catalog::UnknownReason::UnsupportedInventorySource);
    }
    catalog.match_app(app)
}

/// Builds peer-app review candidates for a target. Availability is OS-family only.
/// Unknown target identities may hide existing counterparts; no reinstall is authorized.
pub fn build_plan(
    catalog: &Catalog,
    target: &InventorySnapshot,
    peers: &[InventorySnapshot],
) -> Result<Vec<PlanEntry>, PlanError> {
    if peers.len() > MAX_PEERS {
        return Err(PlanError("peer count limit exceeded".into()));
    }
    let mut input_apps = 0;
    let mut devices: BTreeMap<&str, &DeviceInfo> = BTreeMap::new();
    for snapshot in std::iter::once(target).chain(peers) {
        input_apps += snapshot.apps.len();
        if input_apps > MAX_INPUT_APPS {
            return Err(PlanError("input application count limit exceeded".into()));
        }
        snapshot
            .validate()
            .map_err(|reason| PlanError(reason.into()))?;
        if let Some(previous) = devices.insert(&snapshot.device.id, &snapshot.device)
            && previous != &snapshot.device
        {
            return Err(PlanError(
                "conflicting metadata for duplicate device identity".into(),
            ));
        }
    }

    let mut present = BTreeSet::new();
    let mut target_unknown = false;
    for app in &target.apps {
        match observation(catalog, target, app) {
            MatchOutcome::Matched(product) => {
                present.insert(product.id);
            }
            MatchOutcome::Unknown(_) => target_unknown = true,
        }
    }

    let mut known: BTreeMap<String, PlanEntry> = BTreeMap::new();
    let mut unknown: BTreeMap<(String, String), PlanEntry> = BTreeMap::new();
    for peer in peers
        .iter()
        .filter(|peer| peer.device.id != target.device.id)
    {
        for app in &peer.apps {
            match observation(catalog, peer, app) {
                MatchOutcome::Matched(product) => {
                    let entry = known.entry(product.id.clone()).or_insert_with(|| {
                        let support = product.support(target.device.platform);
                        let (status, mut reason) = if present.contains(&product.id) {
                            (PlanStatus::AlreadyPresent, "Catalog metadata association already appears on target. No reinstall proposed.".into())
                        } else {
                            match support.availability {
                                Availability::Available => (PlanStatus::ReviewRequired, format!("Vendor or package-manager evidence exists for {}. Review CPU architecture, OS version, distribution, license and trusted package mapping before installation. {}", target.device.platform.label(), support.compatibility_note)),
                                Availability::Unavailable => (PlanStatus::Unavailable, format!("Reviewed evidence establishes no official native counterpart for {}. {}", target.device.platform.label(), support.compatibility_note)),
                                Availability::Unknown => (PlanStatus::Unknown, "Official native availability lacks reviewed evidence for target OS family.".into()),
                            }
                        };
                        if target_unknown && status == PlanStatus::ReviewRequired {
                            reason.push_str(" Unknown identities on target may already be this product; check possible duplicates.");
                        }
                        PlanEntry { product_id: Some(product.id.clone()), name: product.name.clone(), status, reason, source_devices: Vec::new() }
                    });
                    entry.source_devices.push(peer.device.id.clone());
                }
                MatchOutcome::Unknown(reason) => {
                    let entry = PlanEntry {
                        product_id: None,
                        name: app.name.clone(),
                        status: PlanStatus::Unknown,
                        reason: format!(
                            "{}. Availability and counterpart identity cannot be inferred.",
                            reason.label()
                        ),
                        source_devices: vec![peer.device.id.clone()],
                    };
                    // App IDs are scoped to devices. Names never merge unknown identities.
                    unknown
                        .entry((peer.device.id.clone(), app.id.clone()))
                        .and_modify(|previous| {
                            if (&entry.name, &entry.reason) < (&previous.name, &previous.reason) {
                                *previous = entry.clone();
                            }
                        })
                        .or_insert(entry);
                }
            }
            if known.len() + unknown.len() > MAX_CANDIDATES {
                return Err(PlanError("candidate count limit exceeded".into()));
            }
        }
    }
    for entry in known.values_mut() {
        entry.source_devices.sort();
        entry.source_devices.dedup();
    }
    Ok(known.into_values().chain(unknown.into_values()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::InstallScope;

    fn windows_app(id: &str, name: &str, publisher: &str) -> InstalledApp {
        InstalledApp {
            id: id.into(),
            name: name.into(),
            publisher: Some(publisher.into()),
            version: None,
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: None,
            origin: None,
        }
    }

    fn vlc() -> InstalledApp {
        windows_app("vlc", "VLC media player", "VideoLAN")
    }

    fn snapshot(number: u128, platform: Platform, apps: Vec<InstalledApp>) -> InventorySnapshot {
        InventorySnapshot {
            device: DeviceInfo {
                id: uuid::Uuid::from_u128(number).to_string(),
                name: format!("Device {number}"),
                platform,
                architecture: "x86_64".into(),
            },
            scanned_at_unix_secs: 0,
            apps,
            warnings: vec![],
        }
    }

    #[test]
    fn deduplicates_review_candidates_across_peers_deterministically() {
        let catalog = Catalog::bundled().unwrap();
        let target = snapshot(1, Platform::Macos, vec![]);
        let mut duplicate = vlc();
        duplicate.id = "second-vlc-install".into();
        let peers = [
            snapshot(3, Platform::Windows, vec![vlc(), duplicate]),
            snapshot(2, Platform::Windows, vec![vlc()]),
        ];
        let plan = build_plan(&catalog, &target, &peers).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].status, PlanStatus::ReviewRequired);
        assert_eq!(plan[0].product_id.as_deref(), Some("vlc-media-player"));
        assert_eq!(
            plan[0].source_devices,
            vec![peers[1].device.id.clone(), peers[0].device.id.clone()]
        );
        assert_eq!(
            plan,
            build_plan(&catalog, &target, &[peers[1].clone(), peers[0].clone()]).unwrap()
        );
        assert!(
            plan[0]
                .reason
                .contains("license and trusted package mapping")
        );
    }

    #[test]
    fn matches_cross_platform_ids_and_never_proposes_reinstall() {
        let catalog = Catalog::bundled().unwrap();
        let mut native = vlc();
        native.name = "VLC".into();
        native.publisher = None;
        native.source = InventorySource::MacosBundle;
        native.native_id = Some("org.videolan.vlc".into());
        let target = snapshot(1, Platform::Macos, vec![native]);
        let peers = [snapshot(2, Platform::Windows, vec![vlc()]), target.clone()];
        let plan = build_plan(&catalog, &target, &peers).unwrap();
        assert_eq!(plan[0].status, PlanStatus::AlreadyPresent);
        assert_eq!(plan[0].source_devices, vec![peers[0].device.id.clone()]);
        assert!(
            build_plan(&catalog, &target, std::slice::from_ref(&target))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn provider_counterparts_are_already_present_across_windows_and_linux() {
        let catalog = Catalog::bundled().unwrap();
        let windows = windows_app("windows-bitwarden", "Bitwarden", "Bitwarden Inc.");
        let mut linux = windows.clone();
        linux.id = "linux-bitwarden".into();
        linux.source = InventorySource::Flatpak;
        linux.publisher = None;
        linux.native_id = Some("com.bitwarden.desktop/x86_64/stable".into());
        let target = snapshot(1, Platform::Linux, vec![linux]);
        let peer = snapshot(2, Platform::Windows, vec![windows]);
        let plan = build_plan(&catalog, &target, &[peer]).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan[0].product_id.as_deref(),
            Some("winget:Bitwarden.Bitwarden")
        );
        assert_eq!(plan[0].status, PlanStatus::AlreadyPresent);
    }

    #[test]
    fn same_names_never_merge_unknown_products_or_prove_target_absence() {
        let catalog = Catalog::bundled().unwrap();
        let unknown = windows_app("unknown", "VLC media player", "Repack vendor");
        let target = snapshot(1, Platform::Windows, vec![unknown.clone()]);
        let peers = [
            snapshot(2, Platform::Windows, vec![vlc(), unknown.clone()]),
            snapshot(3, Platform::Windows, vec![unknown]),
        ];
        let plan = build_plan(&catalog, &target, &peers).unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].status, PlanStatus::ReviewRequired);
        assert!(plan[0].reason.contains("possible duplicates"));
        assert!(
            plan[1..]
                .iter()
                .all(|entry| entry.product_id.is_none() && entry.status == PlanStatus::Unknown)
        );
    }

    #[test]
    fn keeps_unavailable_and_evidence_unknown_separate() {
        let target = snapshot(1, Platform::Linux, vec![]);
        let peer = snapshot(
            2,
            Platform::Windows,
            vec![
                vlc(),
                windows_app("npp", "Notepad++ (64-bit x64)", "Notepad++ Team"),
            ],
        );
        let mut document: serde_json::Value =
            serde_json::from_str(include_str!("../catalog/products.json")).unwrap();
        document["products"][1]["platforms"][2]["availability"] = "unknown".into();
        document["products"][1]["platforms"][2]["evidence"] = serde_json::json!([]);
        let catalog = Catalog::from_json(&document.to_string()).unwrap();
        let plan = build_plan(&catalog, &target, &[peer]).unwrap();
        assert_eq!(plan[0].status, PlanStatus::Unavailable);
        assert_eq!(plan[1].status, PlanStatus::Unknown);
        assert!(build_plan(&catalog, &target, &[]).unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_input_conflicting_devices_and_oversized_plans() {
        let catalog = Catalog::bundled().unwrap();
        let target = snapshot(1, Platform::Windows, vec![]);
        let mut peer = snapshot(2, Platform::Windows, vec![vlc()]);
        peer.apps[0].name.push('\n');
        assert!(build_plan(&catalog, &target, &[peer]).is_err());
        let mut peer = target.clone();
        peer.device.name = "Conflicting device name".into();
        assert!(build_plan(&catalog, &target, &[peer]).is_err());
        assert!(build_plan(&catalog, &target, &vec![target.clone(); MAX_PEERS + 1]).is_err());
        let apps: Vec<_> = (0..10_000)
            .map(|index| windows_app(&format!("app-{index}"), "Unreviewed", "Unreviewed"))
            .collect();
        let peers = [
            snapshot(2, Platform::Windows, apps.clone()),
            snapshot(3, Platform::Windows, apps.clone()),
        ];
        assert!(
            build_plan(&catalog, &target, &peers)
                .unwrap_err()
                .0
                .contains("candidate")
        );
        let peers: Vec<_> = (2..8)
            .map(|id| snapshot(id, Platform::Windows, apps.clone()))
            .collect();
        assert!(
            build_plan(&catalog, &target, &peers)
                .unwrap_err()
                .0
                .contains("input application")
        );
        let mut invalid_target = target;
        invalid_target.device.id = "bad-uuid".into();
        assert!(build_plan(&catalog, &invalid_target, &[]).is_err());
    }

    #[test]
    fn wrong_platform_source_stays_unknown_while_flatpak_ids_are_cross_referenced() {
        let catalog = Catalog::bundled().unwrap();
        let target = snapshot(1, Platform::Windows, vec![]);
        let mut flatpak = vlc();
        flatpak.id = "flatpak-vlc".into();
        flatpak.source = InventorySource::Flatpak;
        flatpak.native_id = Some("org.videolan.VLC/x86_64/stable".into());
        flatpak.origin = Some("flathub".into());
        let peer = snapshot(2, Platform::Linux, vec![vlc(), flatpak]);
        let plan = build_plan(&catalog, &target, &[peer]).unwrap();
        assert_eq!(
            plan.iter()
                .filter(|entry| entry.status == PlanStatus::Unknown)
                .count(),
            1
        );
        assert!(plan.iter().any(
            |entry| entry.product_id.as_deref() == Some("vlc-media-player")
                && entry.status == PlanStatus::ReviewRequired
        ));
    }
}
