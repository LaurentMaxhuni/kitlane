use eframe::egui;
use kitlane_core::{
    catalog::{Availability, Catalog, MatchOutcome},
    model::{AppSettings, DeviceInfo, InventorySnapshot, Platform},
};

use crate::worker::{self, Command, Event, Worker};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Library,
    Devices,
    Settings,
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
}

impl KitlaneApp {
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
        ui.label("Catalog matches use installed name and publisher metadata; they do not authorize installation.");
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
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&app.name).strong()).truncate())
                        .on_hover_text(&app.name);
                    ui.label(app.version.as_deref().unwrap_or("Version unknown"));
                });
                ui.add(
                    egui::Label::new(app.publisher.as_deref().unwrap_or("Publisher unknown"))
                        .truncate(),
                );
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
                ui.heading("Your devices");
                if let Some(device) = &self.device {
                    ui.label(&device.name);
                    ui.label(format!(
                        "{} · {}",
                        device.platform.label(),
                        device.architecture
                    ));
                }
                ui.label("No other devices linked. Linking is not available in the app yet.");
                ui.label("Your inventory stays on this device.");
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
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.content(ui));
                });
                for _ in 0..4 {
                    output.textures_delta.clear();
                    output = ctx.run_ui(egui::RawInput::default(), |ui| {
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
