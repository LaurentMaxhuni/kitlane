use eframe::egui;
use kitlane_core::{
    catalog::{Availability, Catalog, MatchOutcome},
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

    fn devices(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your devices");
        if let Some(device) = &self.device {
            ui.label(format!(
                "{} · {} · {}",
                device.name,
                device.platform.label(),
                device.architecture
            ));
            ui.monospace(&device.id);
            if ui.button("Copy pairing identity").clicked()
                && let Ok(identity) = serde_json::to_string(device)
            {
                ui.ctx().copy_text(identity);
            }
        }
        ui.separator();
        ui.label("Connect to your local Kitlane sync service. Remote-device transport is not available yet.");
        ui.add_enabled_ui(!self.sync.busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Service");
                if ui.text_edit_singleline(&mut self.sync.endpoint).changed() {
                    self.sync.enrolled = false;
                    self.sync.consent = false;
                    self.sync.pairing_token.zeroize();
                    self.peers.clear();
                    self.plans.clear();
                    self.sync.message = None;
                }
            });
            ui.label("Enrolled devices under this owner can read your app names, publishers, versions, and device metadata.");
            ui.checkbox(&mut self.sync.consent, "I agree to share inventory with this service and its enrolled devices");
            let ready = self.device.is_some() && self.worker.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(ready, egui::Button::new("Check saved enrollment")).clicked() {
                    self.send_sync(Command::CheckEnrollment { endpoint: self.sync.endpoint.clone() });
                }
                if ui.add_enabled(ready, egui::Button::new("Forget credential")).clicked() {
                    self.send_sync(Command::Disconnect { endpoint: self.sync.endpoint.clone() });
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.sync.pairing_token).password(true).char_limit(64).hint_text("Owner-issued pairing token"));
            if ui.add_enabled(ready && self.sync.consent && !self.sync.enrolled && self.sync.pairing_token.len() == 64,
                egui::Button::new("Enroll this device")).clicked() {
                let pairing_token = zeroize::Zeroizing::new(std::mem::take(&mut self.sync.pairing_token));
                self.send_sync(Command::Enroll { endpoint: self.sync.endpoint.clone(), pairing_token });
            }
            if ui.add_enabled(ready && self.sync.consent && self.sync.enrolled && self.snapshot.is_some() && !self.scanning,
                egui::Button::new("Sync now")).clicked()
                && let Some(snapshot) = self.snapshot.clone() {
                self.send_sync(Command::Sync { endpoint: self.sync.endpoint.clone(), snapshot: Box::new(snapshot) });
            }
        });
        if self.sync.busy {
            ui.spinner();
            ui.label("Working with the sync service…");
        }
        if let Some(message) = &self.sync.message {
            ui.label(message);
        }
        ui.label("For lost enrollment credentials, ask the service owner for a recovery pairing token. Forgetting a local credential does not revoke it on the service.");
        if !self.peers.is_empty() {
            ui.separator();
            ui.heading("Synced devices");
            for peer in &self.peers {
                ui.label(format!(
                    "{} · {} · {} apps",
                    peer.device.name,
                    peer.device.platform.label(),
                    peer.apps.len()
                ));
            }
        }
        if !self.plans.is_empty() {
            ui.separator();
            ui.heading("Compatibility proposals for this device");
            ui.label("Review only. No programs are installed, updated, or removed.");
            egui::ScrollArea::vertical()
                .id_salt("compatibility-plans")
                .show_rows(ui, 56.0, self.plans.len(), |ui, range| {
                    for index in range {
                        let plan = &self.plans[index];
                        ui.allocate_ui(egui::vec2(ui.available_width(), 56.0), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{} · {}",
                                        plan.name,
                                        plan.status.label()
                                    ))
                                    .strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&plan.name);
                            ui.add(egui::Label::new(&plan.reason).truncate())
                                .on_hover_text(&plan.reason);
                        });
                    }
                });
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

    fn library(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Your app library");
            if ui
                .add_enabled(
                    !self.scanning && self.worker.is_some(),
                    egui::Button::new("Refresh"),
                )
                .clicked()
            {
                self.scan();
            }
            if self.scanning {
                ui.spinner();
                ui.label("Scanning installed apps…");
            }
        });
        ui.label("OS availability uses reviewed vendor evidence. CPU and OS-version compatibility are not checked yet.");
        ui.label("Catalog matches use observed metadata and reviewed identifiers; installation still requires verification.");
        if ui
            .add(egui::TextEdit::singleline(&mut self.query).hint_text("Search apps or publishers"))
            .changed()
        {
            self.filter_rows();
        }
        let Some(snapshot) = &self.snapshot else {
            if !self.scanning {
                ui.label("No inventory available. Refresh to scan this device.");
            }
            return;
        };
        ui.label(format!(
            "{} apps · {} shown",
            snapshot.apps.len(),
            self.rows.len()
        ));
        if self.cached {
            ui.label("Showing saved inventory while this device is checked.");
        }
        if !snapshot.warnings.is_empty() {
            ui.collapsing("Scan coverage and warnings", |ui| {
                for warning in &snapshot.warnings {
                    ui.label(warning);
                }
            });
        }
        ui.separator();
        egui::ScrollArea::vertical().show_rows(ui, 74.0, self.rows.len(), |ui, range| {
            for row in range {
                let index = self.rows[row];
                let app = &snapshot.apps[index];
                let heading = format!(
                    "{} · {}",
                    app.name,
                    app.version.as_deref().unwrap_or("Version unknown")
                );
                let scope = match app.scope {
                    kitlane_core::model::InstallScope::User => "User",
                    kitlane_core::model::InstallScope::Machine => "Machine",
                    kitlane_core::model::InstallScope::Unknown => "Unknown scope",
                };
                let details = match app.source {
                    kitlane_core::model::InventorySource::Flatpak => format!(
                        "Flatpak · {scope} · origin {} (unverified) · {}",
                        app.origin.as_deref().unwrap_or("unknown"),
                        app.native_id.as_deref().unwrap_or("Unknown ID")
                    ),
                    kitlane_core::model::InventorySource::MacosBundle => format!(
                        "macOS bundle · {scope} · {}",
                        app.native_id.as_deref().unwrap_or("Unknown bundle ID")
                    ),
                    kitlane_core::model::InventorySource::WindowsRegistry => format!(
                        "Windows registry · {scope} · {}",
                        app.publisher.as_deref().unwrap_or("Publisher unknown")
                    ),
                    kitlane_core::model::InventorySource::Manual => format!(
                        "Manual · {scope} · {}",
                        app.publisher.as_deref().unwrap_or("Publisher unknown")
                    ),
                };
                ui.allocate_ui(egui::vec2(ui.available_width(), 74.0), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&heading).strong()).truncate())
                        .on_hover_text(&heading);
                    ui.add(egui::Label::new(&details).truncate())
                        .on_hover_text(&details);
                    ui.horizontal(|ui| {
                        if let Some(MatchOutcome::Matched(product)) = self.matches.get(index) {
                            for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
                                let support = product.support(platform);
                                let label = match support.availability {
                                    Availability::Available => "Available",
                                    Availability::Unavailable => "Unavailable",
                                    Availability::Unknown => "Unknown",
                                };
                                let text = format!("{}: {label}", platform.label());
                                if let Some(evidence) = support.evidence.first() {
                                    ui.hyperlink_to(text, &evidence.url)
                                        .on_hover_text(&support.compatibility_note);
                                } else {
                                    ui.label(text);
                                }
                            }
                        } else {
                            ui.label("Cross-platform availability unknown · no catalog match");
                        }
                    });
                    ui.separator();
                });
            }
        });
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        self.receive_events(ui.ctx());
        ui.horizontal(|ui| {
            ui.heading("Kitlane");
            ui.separator();
            for (page, label) in [
                (Page::Library, "Library"),
                (Page::Devices, "Devices"),
                (Page::Settings, "Settings"),
            ] {
                ui.selectable_value(&mut self.page, page, label);
            }
        });
        ui.separator();
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.add_space(12.0);
        match self.page {
            Page::Library => self.library(ui),
            Page::Devices => {
                egui::ScrollArea::vertical()
                    .id_salt("devices-page")
                    .show(ui, |ui| self.devices(ui));
            }
            Page::Settings => {
                ui.heading("Appearance");
                if ui
                    .add_enabled(
                        self.worker.is_none() || self.device.is_some(),
                        egui::Checkbox::new(&mut self.dark, "Dark theme"),
                    )
                    .changed()
                {
                    crate::theme::apply(ui.ctx(), self.dark);
                    if let Some(worker) = &self.worker
                        && let Err(error) = worker.try_send(Command::SaveSettings(AppSettings {
                            dark_theme: self.dark,
                        }))
                    {
                        self.error = Some(format!("Appearance could not be saved: {error}"));
                    }
                }
                ui.separator();
                ui.heading("Automation");
                ui.label("Installation, updates, and AI alternatives are not enabled yet.");
            }
        }
    }
}

impl eframe::App for KitlaneApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| self.content(ui));
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
