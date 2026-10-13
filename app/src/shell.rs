#[path = "account.rs"]
mod account;
#[path = "chrome.rs"]
mod chrome;
#[path = "devices.rs"]
mod devices;
#[path = "library.rs"]
mod library;
#[path = "logos.rs"]
mod logos;
#[path = "native_icons.rs"]
pub(crate) mod native_icons;
#[path = "pairing.rs"]
mod pairing;
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum LibraryFilter {
    #[default]
    All,
    Known,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PairingStep {
    Service,
    Consent,
    Review,
    Done,
}

#[derive(Default)]
struct SyncUi {
    code_input: String,
    consent: bool,
    busy: bool,
    message: Option<String>,
    generated: Option<kitlane_account_client::PairCode>,
    creating_code: bool,
}

impl Drop for SyncUi {
    fn drop(&mut self) {
        self.code_input.zeroize();
    }
}

#[derive(Default)]
pub struct KitlaneApp {
    page: Page,
    dark: bool,
    settings: AppSettings,
    onboarding: bool,
    setup_name: String,
    setup_busy: bool,
    setup_pair: bool,
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
    library_filter: LibraryFilter,
    selected_app: Option<String>,
    local_icons: native_icons::Icons,
    pairing: Option<PairingStep>,
    pairing_transition: Option<PairingStep>,
    transition_key: Option<(Page, bool, u8)>,
    account: Option<kitlane_account_client::Account>,
    auth_busy: bool,
    signing_in: bool,
    paired_devices: Vec<DeviceInfo>,
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
            account: Some(kitlane_account_client::Account {
                account_id: kitlane_core::new_device_id(),
                email: "alex@example.com".into(),
                name: "Alex".into(),
            }),
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
        if (3..=6).contains(&page) {
            app.onboarding = true;
            app.settings.onboarding_step = (page - 3).min(3) as u8;
            app.setup_name = "Studio workstation".into();
        }
        if page >= 7 {
            app.page = Page::Devices;
            app.pairing = Some(match page {
                8 => PairingStep::Consent,
                9 => PairingStep::Review,
                10 => PairingStep::Done,
                _ => PairingStep::Service,
            });
            app.sync.consent = true;
            app.sync.code_input = "ABCD2345".into();
        }
        if page == 1 && state == "normal" {
            let mut peer = app.snapshot.clone().unwrap();
            peer.device.id = kitlane_core::new_device_id();
            peer.device.name = "Design laptop".into();
            peer.device.platform = Platform::Macos;
            peer.scanned_at_unix_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(900);
            app.paired_devices.push(peer.device.clone());
            app.peers.push(peer);
            app.rebuild_plans();
        }
        if page == 11 {
            app.account = None;
            app.onboarding = false;
        }
        if state == "reduced" {
            app.settings.reduced_motion = true;
        }
        if state == "details" {
            app.selected_app = app
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.apps.first().map(|app| app.id.clone()));
        }
        app
    }

    pub fn new(ctx: egui::Context) -> Self {
        let worker = worker::start(ctx);
        Self {
            worker: Some(worker),
            auth_busy: true,
            ..Self::default()
        }
    }

    fn scan(&mut self) {
        if self.account.is_none() {
            return;
        }
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
                    self.setup_name = self.device.as_ref().unwrap().name.clone();
                    self.onboarding = !settings.onboarding_completed;
                    self.settings = settings;
                    crate::theme::apply(ctx, self.dark);
                    if let Some(snapshot) = cached {
                        self.cached = true;
                        self.set_snapshot(snapshot);
                    }
                }
                Event::SetupSaved { device, settings } => {
                    self.setup_busy = false;
                    self.device = Some(device.clone());
                    if let Some(snapshot) = &mut self.snapshot {
                        snapshot.device = device;
                    }
                    self.onboarding = !settings.onboarding_completed;
                    self.dark = settings.dark_theme;
                    let completed = settings.onboarding_completed;
                    self.settings = settings;
                    if completed {
                        self.page = if self.setup_pair {
                            Page::Devices
                        } else {
                            Page::Library
                        };
                        if self.setup_pair {
                            self.open_pairing();
                        }
                        if self.settings.scan_on_startup {
                            self.scan();
                        }
                    }
                }
                Event::SetupFailed(error) => {
                    self.setup_busy = false;
                    self.error = Some(error);
                }
                Event::Scanned(snapshot) => {
                    self.scanning = false;
                    self.cached = false;
                    self.set_snapshot(snapshot);
                }
                Event::IconsLoaded(icons) => {
                    for (id, old) in &self.local_icons {
                        if icons.get(id).is_none_or(|new| new.uri != old.uri) {
                            ctx.forget_image(&old.uri);
                        }
                    }
                    self.local_icons = icons;
                }
                Event::ScanFailed(error) => {
                    self.scanning = false;
                    self.error = Some(error);
                }
                Event::StorageFailed(error) => {
                    self.auth_busy = false;
                    self.scanning = false;
                    self.error = Some(error);
                }
                Event::AccountReady(account) => {
                    self.auth_busy = false;
                    self.signing_in = false;
                    self.error = None;
                    self.account = Some(account);
                    self.send_sync(Command::RefreshDevices);
                    if self.settings.onboarding_completed && self.settings.scan_on_startup {
                        self.scan();
                    }
                }
                Event::SignedOut(message) => {
                    self.clear_account();
                    self.auth_busy = false;
                    self.signing_in = false;
                    self.error = message;
                }
                Event::CodeGenerated(code) => {
                    self.sync.busy = false;
                    self.sync.generated = Some(code);
                    self.sync.message = None;
                }
                Event::Devices(devices) => {
                    let connected = devices
                        .iter()
                        .any(|d| !self.paired_devices.iter().any(|old| old.id == d.id));
                    self.sync.busy = false;
                    self.paired_devices = devices;
                    self.peers
                        .retain(|peer| self.paired_devices.iter().any(|d| d.id == peer.device.id));
                    self.rebuild_plans();
                    if self.pairing == Some(PairingStep::Review)
                        || (connected && self.sync.creating_code && self.pairing.is_some())
                    {
                        self.pairing = Some(PairingStep::Done);
                        self.sync.message = Some(
                            "Device connected. Sync when you are ready to share inventory.".into(),
                        );
                    }
                }
                Event::Synced(peers) => {
                    self.sync.busy = false;
                    self.peers = peers
                        .into_iter()
                        .filter(|peer| {
                            self.device
                                .as_ref()
                                .is_none_or(|device| device.id != peer.device.id)
                        })
                        .collect();
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
        if self.account.is_none()
            || (matches!(&command, Command::Sync { .. }) && !self.sync.consent)
        {
            self.sync.message = Some("Sign in and choose inventory sharing before syncing.".into());
            return;
        }
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
            .filter_map(|(index, name)| {
                let known = matches!(self.matches.get(index), Some(MatchOutcome::Matched(_)));
                let visible = match self.library_filter {
                    LibraryFilter::All => true,
                    LibraryFilter::Known => known,
                    LibraryFilter::Unknown => !known,
                };
                (visible && name.contains(&query)).then_some(index)
            })
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
    fn account() -> kitlane_account_client::Account {
        kitlane_account_client::Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        }
    }

    #[test]
    fn signed_out_startup_cannot_scan_or_unlock_workspace() {
        let ctx = egui::Context::default();
        let (worker, commands, events) = worker::test_worker();
        let mut app = KitlaneApp {
            worker: Some(worker),
            ..Default::default()
        };
        events
            .send(Event::Ready {
                device: DeviceInfo {
                    id: kitlane_core::new_device_id(),
                    name: "Test".into(),
                    platform: Platform::Windows,
                    architecture: "x86_64".into(),
                },
                settings: AppSettings {
                    onboarding_completed: true,
                    ..Default::default()
                },
                cached: None,
            })
            .unwrap();
        app.receive_events(&ctx);
        app.scan();
        assert!(
            commands.try_recv().is_err(),
            "Scanning requires verified Google login"
        );
    }

    #[test]
    fn signout_preserves_local_inventory_and_preferences_but_clears_cloud_consent() {
        let mut app = KitlaneApp::preview(1, true, "normal");
        app.sync.consent = true;
        app.sync.code_input = "ABCD2345".into();
        let local = app.snapshot.clone();
        let settings = app.settings.clone();
        app.clear_account();
        assert!(app.account.is_none());
        assert!(app.paired_devices.is_empty());
        assert!(app.peers.is_empty());
        assert!(!app.sync.consent);
        assert!(app.sync.code_input.is_empty());
        assert_eq!(app.snapshot, local);
        assert_eq!(app.settings, settings);
    }

    #[test]
    fn login_and_pairing_do_not_upload_and_sync_requires_fresh_consent() {
        let ctx = egui::Context::default();
        let (worker, commands, events) = worker::test_worker();
        let mut app = KitlaneApp::preview(0, false, "normal");
        app.worker = Some(worker);
        app.account = None;
        events.send(Event::AccountReady(account())).unwrap();
        app.receive_events(&ctx);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::RefreshDevices
        ));
        assert!(commands.try_recv().is_err());
        events
            .send(Event::Devices(vec![DeviceInfo {
                id: kitlane_core::new_device_id(),
                name: "Peer".into(),
                platform: Platform::Windows,
                architecture: "x86_64".into(),
            }]))
            .unwrap();
        app.receive_events(&ctx);
        assert_eq!(app.paired_devices.len(), 1);
        assert!(app.peers.is_empty());
        assert!(commands.try_recv().is_err());
        app.send_sync(Command::Sync {
            snapshot: Box::new(app.snapshot.clone().unwrap()),
        });
        assert!(commands.try_recv().is_err());
        app.sync.consent = true;
        app.send_sync(Command::Sync {
            snapshot: Box::new(app.snapshot.clone().unwrap()),
        });
        assert!(matches!(commands.try_recv().unwrap(), Command::Sync { .. }));
    }

    #[test]
    fn reduced_motion_loading_does_not_schedule_continuous_repaint() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, true);
        egui_extras::install_image_loaders(&ctx);
        let mut app = KitlaneApp::preview(0, true, "loading");
        app.settings.reduced_motion = true;
        for frame in 0..6 {
            let input = egui::RawInput {
                time: Some(frame as f64),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.content(ui));
            output.textures_delta.clear();
            if frame == 5 {
                assert!(
                    output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                        > std::time::Duration::ZERO
                );
            }
        }
        assert_eq!(ctx.global_style().animation_time, 0.0);
    }

    #[test]
    fn filters_combine_with_search_and_keep_unknown_apps() {
        let mut app = KitlaneApp::preview(0, false, "normal");
        app.library_filter = LibraryFilter::Known;
        app.filter_rows();
        assert_eq!(app.rows.len(), 3);
        app.library_filter = LibraryFilter::Unknown;
        app.filter_rows();
        assert_eq!(app.rows.len(), 4);
        app.query = "figma".into();
        app.filter_rows();
        assert_eq!(app.rows.len(), 1);
        app.library_filter = LibraryFilter::Known;
        app.filter_rows();
        assert!(app.rows.is_empty());
    }

    #[test]
    fn onboarding_blocks_scans_until_successful_completion() {
        for scan_on_startup in [false, true] {
            let ctx = egui::Context::default();
            let (worker, commands, events) = worker::test_worker();
            let mut app = KitlaneApp {
                worker: Some(worker),
                ..Default::default()
            };
            let device = DeviceInfo {
                id: kitlane_core::new_device_id(),
                name: "Test PC".into(),
                platform: Platform::Windows,
                architecture: "x86_64".into(),
            };
            let mut settings = AppSettings {
                onboarding_step: 2,
                scan_on_startup,
                ..Default::default()
            };
            events
                .send(Event::Ready {
                    device: device.clone(),
                    settings: settings.clone(),
                    cached: None,
                })
                .unwrap();
            app.receive_events(&ctx);
            assert!(app.onboarding);
            assert_eq!(app.settings.onboarding_step, 2);
            assert!(commands.try_recv().is_err());
            events.send(Event::AccountReady(account())).unwrap();
            app.receive_events(&ctx);
            assert!(matches!(
                commands.try_recv().unwrap(),
                Command::RefreshDevices
            ));
            app.setup_busy = true;
            events.send(Event::SetupFailed("Disk full".into())).unwrap();
            app.receive_events(&ctx);
            assert!(app.onboarding);
            assert!(!app.setup_busy);
            assert!(commands.try_recv().is_err());
            settings.onboarding_completed = true;
            events.send(Event::SetupSaved { device, settings }).unwrap();
            app.receive_events(&ctx);
            assert!(!app.onboarding);
            assert_eq!(app.scanning, scan_on_startup);
            if scan_on_startup {
                assert!(matches!(commands.try_recv().unwrap(), Command::Scan));
            } else {
                assert!(commands.try_recv().is_err());
            }
        }
    }

    #[test]
    fn onboarding_renders_all_steps_in_both_themes_and_small_windows() {
        for dark in [false, true] {
            for step in 0..4 {
                for width in [480.0, 1440.0] {
                    let ctx = egui::Context::default();
                    crate::theme::install_fonts(&ctx);
                    crate::theme::apply(&ctx, dark);
                    egui_extras::install_image_loaders(&ctx);
                    let mut app = KitlaneApp::preview(step + 3, dark, "normal");
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| app.content(ui));
                    assert!(!output.shapes.is_empty());
                    output.textures_delta.clear();
                    assert!(app.onboarding);
                    assert!(!app.scanning);
                }
            }
        }
    }

    #[test]
    fn all_pages_render_in_both_themes_without_continuous_repaint() {
        for width in [480.0, 900.0, 1440.0] {
            let ctx = egui::Context::default();
            egui_extras::install_image_loaders(&ctx);
            crate::theme::install_fonts(&ctx);
            for dark in [false, true] {
                crate::theme::apply(&ctx, dark);
                for page in [Page::Library, Page::Devices, Page::Settings] {
                    let mut app = KitlaneApp {
                        page,
                        dark,
                        account: Some(account()),
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
                    app.paired_devices = vec![peer.device.clone()];
                    app.peers = vec![peer];
                    app.rebuild_plans();
                    assert_eq!(app.plans.len(), 1);
                    let input = || egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 720.0),
                        )),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input(), |ui| {
                        app.content(ui);
                    });
                    for frame in 0..8 {
                        output.textures_delta.clear();
                        let mut timed_input = input();
                        timed_input.time = Some(frame as f64 + 1.0);
                        output = ctx.run_ui(timed_input, |ui| {
                            app.content(ui);
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
    }

    #[test]
    fn keyboard_navigation_and_appearance_work_without_a_worker() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, false);
        egui_extras::install_image_loaders(&ctx);
        let mut app = KitlaneApp::preview(0, false, "normal");
        let mut time = 0.0;
        let mut press = |app: &mut KitlaneApp, key: Option<egui::Key>| {
            time += 1.0;
            let mut input = egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            if let Some(key) = key {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                });
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: false,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(input, |ui| app.content(ui));
            output.textures_delta.clear();
        };
        press(&mut app, None);
        // Window controls precede sidebar navigation in keyboard focus order.
        for _ in 0..4 {
            press(&mut app, Some(egui::Key::Tab));
        }
        press(&mut app, Some(egui::Key::Tab));
        assert!(ctx.memory(|memory| memory.focused()).is_some());
        press(&mut app, Some(egui::Key::Tab));
        press(&mut app, Some(egui::Key::Enter));
        assert!(app.page == Page::Devices);
        press(&mut app, Some(egui::Key::Tab));
        press(&mut app, Some(egui::Key::Enter));
        assert!(app.page == Page::Settings);
        press(&mut app, Some(egui::Key::Tab));
        press(&mut app, Some(egui::Key::Tab));
        press(&mut app, Some(egui::Key::Enter));
        assert!(app.dark);
        assert!(app.worker.is_none());
        assert!(app.sync.code_input.is_empty());
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
