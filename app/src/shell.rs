#[path = "view.rs"]
mod view;

use eframe::egui;
use kitlane_core::{
    catalog::{Catalog, MatchOutcome},
    model::{AppSettings, DeviceInfo, InventorySnapshot, Platform},
    planner::{self, PlanEntry},
};
use zeroize::Zeroize;

use crate::worker::{self, Command, Event, Worker};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Library,
    Devices,
    Settings,
}

struct SyncUi {
    endpoint: String,
    pairing_token: String,
    consent: bool,
    enrolled: bool,
    busy: bool,
    message: Option<String>,
}

impl Default for SyncUi {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8787".into(),
            pairing_token: String::new(),
            consent: false,
            enrolled: false,
            busy: false,
            message: None,
        }
    }
}

impl Drop for SyncUi {
    fn drop(&mut self) {
        self.pairing_token.zeroize();
    }
}

#[derive(Default)]
pub struct KitlaneApp {
    page: Page,
    dark: bool,
    worker: Option<Worker>,
    device: Option<DeviceInfo>,
    snapshot: Option<InventorySnapshot>,
    matches: Vec<MatchOutcome>,
    rows: Vec<usize>,
    search_names: Vec<String>,
    query: String,
    scanning: bool,
    cached: bool,
    error: Option<String>,
    sync: SyncUi,
    peers: Vec<InventorySnapshot>,
    plans: Vec<PlanEntry>,
}

impl KitlaneApp {
    #[cfg(debug_assertions)]
    #[allow(dead_code)] // Used only by the diagnostic preview binary.
    pub(crate) fn preview(page: usize, dark: bool, state: &str) -> Self {
        use kitlane_core::model::{InstallScope, InstalledApp, InventorySource};
        let device = DeviceInfo {
            id: kitlane_core::new_device_id(),
            name: "Studio workstation".into(),
            platform: Platform::Windows,
            architecture: "x86_64".into(),
        };
        let mut app = Self {
            page: match page {
                1 => Page::Devices,
                2 => Page::Settings,
                _ => Page::Library,
            },
            dark,
            device: Some(device.clone()),
            ..Self::default()
        };
        let mut apps: Vec<_> = [
            (
                "Microsoft Visual Studio Code (User)",
                "Microsoft Corporation",
                "1.105.0",
            ),
            ("VLC media player", "VideoLAN", "3.0.21"),
            ("Notepad++ (64-bit x64)", "Notepad++ Team", "8.8.5"),
            ("Figma", "Figma, Inc.", "124.0"),
            ("Spotify", "Spotify AB", "1.2.73"),
            ("Blender", "Blender Foundation", "4.5.0"),
            ("Obsidian", "Dynalist Inc.", "1.9.12"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, publisher, version))| InstalledApp {
            id: format!("preview-{i}"),
            name: name.into(),
            publisher: Some(publisher.into()),
            version: Some(version.into()),
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: None,
            origin: None,
        })
        .collect();
        if state == "empty" {
            apps.clear();
        }
        if state == "long" {
            apps[0].name = "A very long application name ".repeat(15);
            apps[0].publisher = Some("An unusually long publisher name ".repeat(15));
            apps[0].version = Some("2026.10.11.long-version".repeat(5));
        }
        app.set_snapshot(InventorySnapshot {
            device,
            scanned_at_unix_secs: 1,
            apps,
            warnings: Vec::new(),
        });
        if state == "loading" {
            app.scanning = true;
        }
        if state == "error" {
            app.error = Some("Inventory scan failed. Your saved library is still available. Try refreshing again.".into());
        }
        app
    }

    pub fn new(ctx: egui::Context) -> Self {
        let worker = worker::start(ctx);
        let mut app = Self {
            worker: Some(worker),
            ..Self::default()
        };
        app.scan();
        app
    }

    fn scan(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.try_send(Command::Scan) {
            Ok(()) => {
                self.scanning = true;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Cannot start inventory scan: {error}"));
            }
        }
    }

    fn receive_events(&mut self, ctx: &egui::Context) {
        loop {
            let event = self
                .worker
                .as_ref()
                .and_then(|worker| worker.events.try_recv().ok());
            let Some(event) = event else {
                break;
            };
            match event {
                Event::Ready {
                    device,
                    settings,
                    cached,
                } => {
                    self.device = Some(device);
                    self.dark = settings.dark_theme;
                    crate::theme::apply(ctx, self.dark);
                    if let Some(snapshot) = cached {
                        self.cached = true;
                        self.set_snapshot(snapshot);
                    }
                }
                Event::Scanned(snapshot) => {
                    self.scanning = false;
                    self.cached = false;
                    self.set_snapshot(snapshot);
                }
                Event::ScanFailed(error) => {
                    self.scanning = false;
                    self.error = Some(error);
                }
                Event::StorageFailed(error) => {
                    self.scanning = false;
                    self.error = Some(error);
                }
                Event::EnrollmentState(enrolled) => {
                    self.sync.busy = false;
                    self.sync.enrolled = enrolled;
                    self.peers.clear();
                    self.plans.clear();
                    self.sync.message = Some(
                        if enrolled {
                            "Credential saved. Sync runs only when you click Sync now."
                        } else {
                            "No saved credential. Forgetting it does not revoke server access."
                        }
                        .into(),
                    );
                }
                Event::Synced(peers) => {
                    self.sync.busy = false;
                    self.peers = peers;
                    self.rebuild_plans();
                    self.sync.message =
                        Some("Inventory shared and peer snapshots refreshed.".into());
                }
                Event::SyncFailed(error) => {
                    self.sync.busy = false;
                    self.sync.message = Some(error);
                }
            }
        }
    }

    fn set_snapshot(&mut self, snapshot: InventorySnapshot) {
        self.search_names = snapshot
            .apps
            .iter()
            .map(|app| {
                format!("{} {}", app.name, app.publisher.as_deref().unwrap_or("")).to_lowercase()
            })
            .collect();
        match Catalog::bundled() {
            Ok(catalog) => {
                self.matches = snapshot
                    .apps
                    .iter()
                    .map(|app| catalog.match_app(app))
                    .collect();
            }
            Err(error) => {
                self.matches.clear();
                self.error = Some(format!("Catalog unavailable: {error}"));
            }
        }
        self.snapshot = Some(snapshot);
        self.filter_rows();
        self.rebuild_plans();
    }

    fn rebuild_plans(&mut self) {
        self.plans.clear();
        if let Some(snapshot) = &self.snapshot {
            match Catalog::bundled() {
                Ok(catalog) => match planner::build_plan(&catalog, snapshot, &self.peers) {
                    Ok(plans) => self.plans = plans,
                    Err(error) => {
                        self.error = Some(format!("Cannot build compatibility plan: {error}"))
                    }
                },
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn send_sync(&mut self, command: Command) {
        if let Some(worker) = &self.worker {
            match worker.try_send(command) {
                Ok(()) => {
                    self.sync.busy = true;
                    self.sync.message = None;
                }
                Err(_) => {
                    self.sync.message =
                        Some("Background worker unavailable or busy. Try again.".into())
                }
            }
        }
    }

    fn filter_rows(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.rows = self
            .search_names
            .iter()
            .enumerate()
            .filter_map(|(index, name)| name.contains(&query).then_some(index))
            .collect();
    }
}

impl eframe::App for KitlaneApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.content(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pages_render_in_both_themes_without_continuous_repaint() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for dark in [false, true] {
            crate::theme::apply(&ctx, dark);
            for page in [Page::Library, Page::Devices, Page::Settings] {
                let mut app = KitlaneApp {
                    page,
                    dark,
                    ..Default::default()
                };
                app.set_snapshot(InventorySnapshot {
                    device: DeviceInfo {
                        id: kitlane_core::new_device_id(),
                        name: "Test device".into(),
                        platform: Platform::Windows,
                        architecture: "x86_64".into(),
                    },
                    scanned_at_unix_secs: 1,
                    apps: vec![kitlane_core::model::InstalledApp {
                        id: "vlc-test".into(),
                        name: "VLC media player".into(),
                        publisher: Some("VideoLAN".into()),
                        version: Some("3.0".into()),
                        scope: kitlane_core::model::InstallScope::Machine,
                        source: kitlane_core::model::InventorySource::WindowsRegistry,
                        native_id: None,
                        origin: None,
                    }],
                    warnings: vec!["Partial inventory coverage".into()],
                });
                assert!(matches!(app.matches[0], MatchOutcome::Matched(_)));
                let mut peer = app.snapshot.clone().unwrap();
                app.device = Some(peer.device.clone());
                peer.device.id = kitlane_core::new_device_id();
                peer.device.name = "Peer device".into();
                peer.apps[0].name = "Notepad++ (64-bit x64)".into();
                peer.apps[0].publisher = Some("Notepad++ Team".into());
                app.peers = vec![peer];
                app.rebuild_plans();
                assert_eq!(app.plans.len(), 1);
                let input = || egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 720.0),
                    )),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.content(ui));
                });
                for _ in 0..4 {
                    output.textures_delta.clear();
                    output = ctx.run_ui(input(), |ui| {
                        egui::CentralPanel::default().show(ui, |ui| app.content(ui));
                    });
                }
                output.textures_delta.clear();
                assert!(!output.shapes.is_empty());
                assert!(
                    output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                        > std::time::Duration::ZERO
                );
            }
        }
    }

    #[test]
    fn inventory_search_matches_name_and_publisher_without_changing_records() {
        let mut app = KitlaneApp {
            search_names: vec!["editor acme".into(), "player example".into()],
            ..Default::default()
        };
        app.query = " ACME ".into();
        app.filter_rows();
        assert_eq!(app.rows, [0]);
        app.query.clear();
        app.filter_rows();
        assert_eq!(app.rows, [0, 1]);
    }
}
