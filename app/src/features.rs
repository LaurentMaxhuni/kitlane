use super::view::{caption, card};
use super::*;
use crate::theme;
use kitlane_core::{
    maintenance::{MaintenanceAction, MaintenanceState, ProgramPolicy},
    model::InstalledApp,
    suggestions::{Preferences, Suggestion},
};
use std::{
    fs::File,
    sync::mpsc,
    time::{Duration, Instant},
};

enum FeatureCommand {
    Refresh(Option<DeviceInfo>),
    Policy {
        device: DeviceInfo,
        programs: Vec<ProgramPolicy>,
        kitlane: bool,
    },
    UpdateNow {
        device: DeviceInfo,
        program: ProgramPolicy,
    },
    Queue(MaintenanceAction),
    SaveKey(zeroize::Zeroizing<String>),
    ForgetKey,
    Suggest {
        app: InstalledApp,
        preferences: Preferences,
        platform: Platform,
    },
    Accept {
        app: String,
        candidate: String,
    },
    Acknowledge(String),
}

enum FeatureEvent {
    State(MaintenanceState, bool),
    Suggestions(String, Result<Vec<Suggestion>, String>),
    Error(String),
}

struct FeatureWorker {
    commands: Option<mpsc::SyncSender<FeatureCommand>>,
    events: mpsc::Receiver<FeatureEvent>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for FeatureWorker {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl FeatureWorker {
    fn start(ctx: egui::Context) -> Self {
        let (commands, receiver) = mpsc::sync_channel(8);
        let (sender, events) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let ai = crate::ai_client::Client::new().map(std::sync::Arc::new);
            while let Ok(command) = receiver.recv() {
                if let FeatureCommand::Suggest {
                    app,
                    preferences,
                    platform,
                } = command
                {
                    let sender = sender.clone();
                    let ctx = ctx.clone();
                    let ai = ai
                        .as_ref()
                        .map(std::sync::Arc::clone)
                        .map_err(ToString::to_string);
                    // AI requests never hold the policy worker or delay shutdown writes.
                    std::thread::spawn(move || {
                        let result = ai.and_then(|client| {
                            client
                                .suggest(&app, &preferences, platform)
                                .map_err(|e| e.to_string())
                        });
                        let _ = sender.send(FeatureEvent::Suggestions(app.id, result));
                        ctx.request_repaint();
                    });
                    continue;
                }
                let result = (|| -> Result<MaintenanceState, String> {
                    match command {
                        FeatureCommand::Refresh(device) => {
                            let state = crate::maintenance_agent::load()?;
                            if let Some(device) = device
                                && state.device.as_ref() != Some(&device)
                            {
                                crate::maintenance_agent::mutate(|state| {
                                    state.device = Some(device);
                                    Ok(())
                                })?;
                            }
                        }
                        FeatureCommand::Policy {
                            device,
                            programs,
                            kitlane,
                        } => {
                            crate::maintenance_agent::mutate(|state| {
                                state.device = Some(device);
                                state.programs = programs;
                                state.kitlane_auto_update = kitlane;
                                state.next_check_at = 0;
                                Ok(())
                            })?;
                            crate::maintenance_agent::ensure_running()?;
                        }
                        FeatureCommand::UpdateNow { device, program } => {
                            crate::maintenance_agent::mutate(|state| {
                                state.device = Some(device);
                                if !state.programs.iter().any(|p| {
                                    p.app_id == program.app_id && p.winget_id == program.winget_id
                                }) {
                                    state.programs.push(program.clone());
                                }
                                state.enqueue(MaintenanceAction::UpdateProgram {
                                    app_id: program.app_id,
                                    winget_id: program.winget_id,
                                })?;
                                Ok(())
                            })?;
                            crate::maintenance_agent::ensure_running()?;
                        }
                        FeatureCommand::Queue(action) => {
                            crate::maintenance_agent::enqueue(action)?;
                            crate::maintenance_agent::ensure_running()?;
                        }
                        FeatureCommand::SaveKey(key) => ai
                            .as_ref()
                            .map_err(ToString::to_string)?
                            .save_key(key)
                            .map_err(|e| e.to_string())?,
                        FeatureCommand::ForgetKey => ai
                            .as_ref()
                            .map_err(ToString::to_string)?
                            .clear_key()
                            .map_err(|e| e.to_string())?,
                        FeatureCommand::Accept { app, candidate } => {
                            crate::maintenance_agent::mutate(|state| {
                                state.accepted_alternatives.insert(app, candidate);
                                Ok(())
                            })?;
                        }
                        FeatureCommand::Acknowledge(id) => {
                            crate::maintenance_agent::mutate(|state| {
                                let job = state.jobs.iter_mut().find(|job| job.id == id && job.status == kitlane_core::maintenance::JobStatus::Indeterminate).ok_or("Interrupted job is no longer awaiting review")?;
                                job.status = kitlane_core::maintenance::JobStatus::Cancelled;
                                job.outcome = "User reviewed interrupted work and permitted a fresh inventory check.".into();
                                job.updated_at = kitlane_core::maintenance::now();
                                state.next_check_at = 0;
                                Ok(())
                            })?;
                            crate::maintenance_agent::ensure_running()?;
                        }
                        FeatureCommand::Suggest { .. } => unreachable!(),
                    }
                    crate::maintenance_agent::load()
                })();
                let event = match result {
                    Ok(state) => match ai
                        .as_ref()
                        .map_err(ToString::to_string)
                        .and_then(|client| client.configured().map_err(|e| e.to_string()))
                    {
                        Ok(configured) => FeatureEvent::State(state, configured),
                        Err(error) => {
                            let _ = sender.send(FeatureEvent::Error(error));
                            FeatureEvent::State(state, false)
                        }
                    },
                    Err(error) => FeatureEvent::Error(error),
                };
                if sender.send(event).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            commands: Some(commands),
            events,
            handle: Some(handle),
        }
    }
}

#[derive(Default)]
pub(super) struct Features {
    worker: Option<FeatureWorker>,
    state: MaintenanceState,
    configured: bool,
    key_input: String,
    consent: bool,
    preferences: Preferences,
    target_platform: Option<Platform>,
    suggestions: Vec<Suggestion>,
    suggestion_for: Option<String>,
    ai_busy: bool,
    message: Option<String>,
    last_poll: Option<Instant>,
    _ui_lock: Option<File>,
    initialized: bool,
    rescan_needed: bool,
    #[cfg(debug_assertions)]
    pub(super) focus_preview: bool,
}

impl Drop for Features {
    fn drop(&mut self) {
        self.key_input.zeroize();
    }
}

impl Features {
    pub(super) fn start(ctx: egui::Context) -> Self {
        let (lock, message) = match crate::maintenance_agent::hold_ui_lock() {
            Ok(lock) => (Some(lock), None),
            Err(error) => (None, Some(error)),
        };
        let mut features = Self::default();
        features.worker = Some(FeatureWorker::start(ctx));
        features._ui_lock = lock;
        features.message = message;
        features
    }

    fn send(&mut self, command: FeatureCommand) -> bool {
        let result = self
            .worker
            .as_ref()
            .and_then(|worker| worker.commands.as_ref())
            .ok_or("Background worker unavailable")
            .and_then(|sender| {
                sender
                    .try_send(command)
                    .map_err(|_| "Background worker busy; try again")
            });
        if let Err(error) = result {
            self.message = Some(error.into());
            false
        } else {
            true
        }
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context, device: Option<&DeviceInfo>) -> bool {
        while let Some(event) = self
            .worker
            .as_ref()
            .and_then(|worker| worker.events.try_recv().ok())
        {
            match event {
                FeatureEvent::State(state, configured) => {
                    if self.initialized
                        && state.jobs.iter().any(|job| {
                            job.kind == kitlane_core::maintenance::JobKind::ProgramUpdate
                                && job.status == kitlane_core::maintenance::JobStatus::Succeeded
                                && !self
                                    .state
                                    .jobs
                                    .iter()
                                    .any(|old| old.id == job.id && old.status == job.status)
                        })
                    {
                        self.rescan_needed = true;
                    }
                    self.initialized = true;
                    self.state = state;
                    self.configured = configured;
                }
                FeatureEvent::Suggestions(app, result) => {
                    self.ai_busy = false;
                    self.suggestion_for = Some(app);
                    match result {
                        Ok(suggestions) => {
                            self.message = if suggestions.is_empty() {
                                Some("No confident alternative met your requirements.".into())
                            } else {
                                None
                            };
                            self.suggestions = suggestions;
                        }
                        Err(error) => {
                            self.suggestions.clear();
                            self.message = Some(error);
                        }
                    }
                }
                FeatureEvent::Error(error) => self.message = Some(error),
            }
        }
        if self.worker.is_some()
            && self
                .last_poll
                .is_none_or(|last| last.elapsed() > Duration::from_secs(2))
        {
            self.last_poll = Some(Instant::now());
            self.send(FeatureCommand::Refresh(device.cloned()));
        }
        if self.worker.is_some() {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        std::mem::take(&mut self.rescan_needed)
    }
}

impl KitlaneApp {
    pub(super) fn automation_settings(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        ui.add_space(16.0);
        #[cfg(debug_assertions)]
        if std::mem::take(&mut self.features.focus_preview) {
            ui.scroll_to_cursor(Some(egui::Align::Min));
        }
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Automatic updates");
            caption(ui, "Choose apps below to update them in the background, including after Kitlane closes. New agreements, pins and restarts need your attention.", p);
            ui.label(format!("Kitlane {}", env!("CARGO_PKG_VERSION")));
            let supported = cfg!(all(windows, target_arch = "x86_64"));
            let self_updates_ready = crate::self_update::signing_configured();
            if !supported { caption(ui, "Automatic updates are currently available on Windows x64. Other platforms require manual updates.", p); }
            let mut kitlane = self.features.state.kitlane_auto_update;
            if ui.add_enabled(supported && self_updates_ready && self.features.worker.is_some(), egui::Checkbox::new(&mut kitlane, "Keep Kitlane up to date")).changed()
                && let Some(device) = self.device.clone()
                && self.features.send(FeatureCommand::Policy { device, programs: self.features.state.programs.clone(), kitlane }) {
                self.features.state.kitlane_auto_update = kitlane;
            }
            if !self_updates_ready { caption(ui, "Kitlane self-updates are awaiting trusted release-signing setup.", p); }
            caption(ui, "Kitlane installs signed updates after its window closes. Automatic checks run about every six hours.", p);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(supported && self.features.worker.is_some(), egui::Button::new("Check Kitlane now")).clicked() { self.features.send(FeatureCommand::Queue(MaintenanceAction::CheckKitlane)); }
                if ui.add_enabled(supported && !self.features.state.programs.is_empty() && self.features.worker.is_some(), egui::Button::new("Check selected apps")).clicked() { self.features.send(FeatureCommand::Queue(MaintenanceAction::CheckPrograms)); }
            });
            caption(ui, &self.features.state.agent_status, p);
            if let Some(snapshot) = self.snapshot.clone() {
                ui.collapsing("Choose apps for automatic updates", |ui| {
                    let mut any = false;
                    for (app, matched) in snapshot.apps.iter().zip(&self.matches) {
                        let MatchOutcome::Matched(product) = matched else { continue; };
                        let Some(winget_id) = &product.winget_id else { continue; };
                        any = true;
                        let mut enabled = self.features.state.programs.iter().any(|policy| policy.app_id == app.id && policy.winget_id == *winget_id && policy.enabled);
                        if ui.add_enabled(supported && self.features.worker.is_some(), egui::Checkbox::new(&mut enabled, &app.name)).changed() {
                            let mut programs = self.features.state.programs.clone();
                            programs.retain(|policy| policy.app_id != app.id);
                            programs.push(ProgramPolicy { app_id: app.id.clone(), winget_id: winget_id.clone(), display_name: app.name.clone(), enabled });
                            if self.features.send(FeatureCommand::Policy { device: snapshot.device.clone(), programs: programs.clone(), kitlane: self.features.state.kitlane_auto_update }) { self.features.state.programs = programs; }
                        }
                    }
                    if !any { ui.label("Rescan your library to find supported program identifiers."); }
                });
            }
            ui.collapsing("Update activity", |ui| {
                if self.features.state.jobs.is_empty() { ui.label("No update jobs yet."); }
                for job in self.features.state.jobs.clone().into_iter().rev().take(12) {
                    ui.collapsing(format!("{} · {:?}", job.display_name, job.status), |ui| {
                        ui.label(&job.outcome);
                        if let Some(version) = &job.old_version { ui.label(format!("Previous version: {version}")); }
                        if let Some(version) = &job.new_version { ui.label(format!("Result version: {version}")); }
                        caption(ui, &job.provenance, p);
                        if job.status == kitlane_core::maintenance::JobStatus::Indeterminate {
                            caption(ui, "Check this app and any installer still running before allowing another attempt.", p);
                            if ui.button("I reviewed this interruption; allow a fresh check").clicked() {
                                self.features.send(FeatureCommand::Acknowledge(job.id.clone()));
                            }
                        }
                    });
                }
            });
        });
        ui.add_space(16.0);
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("AI recommendations");
            caption(ui, "Open an app's details in your Library to compare alternatives for your workflow. Each request sends only that app, your requirements and relevant candidate facts to TypeSafe.", p);
            ui.label(if self.features.configured { "API key available" } else { "Add your TypeSafe API key to enable suggestions" });
            ui.add(egui::TextEdit::singleline(&mut self.features.key_input).password(true).hint_text("TypeSafe API key").desired_width(f32::INFINITY).char_limit(4096));
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.features.key_input.trim().is_empty() && self.features.worker.is_some(), egui::Button::new("Save key securely")).clicked() {
                    let key = zeroize::Zeroizing::new(std::mem::take(&mut self.features.key_input));
                    self.features.send(FeatureCommand::SaveKey(key));
                }
                if ui.add_enabled(self.features.worker.is_some(), egui::Button::new("Forget saved key")).clicked() { self.features.send(FeatureCommand::ForgetKey); }
            });
            caption(ui, "Saved keys stay in your operating system's credential store. A key supplied by your environment remains available after Forget.", p);
        });
        if let Some(message) = &self.features.message {
            ui.add_space(8.0);
            caption(ui, message, p);
        }
    }

    pub(super) fn suggestions_view(&mut self, ui: &mut egui::Ui, app: &InstalledApp) {
        let p = theme::palette(self.dark);
        ui.add_space(12.0);
        #[cfg(debug_assertions)]
        if std::mem::take(&mut self.features.focus_preview) {
            ui.scroll_to_cursor(Some(egui::Align::Min));
        }
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Find an alternative");
            let mut platform = self.features.target_platform.unwrap_or_else(|| self.device.as_ref().map_or(Platform::Windows, |device| device.platform));
            let original_platform = platform;
            let changed = ui.add_enabled_ui(!self.features.ai_busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Alternatives for");
                    egui::ComboBox::from_id_salt(("alternative-platform", &app.id)).selected_text(platform.label()).show_ui(ui, |ui| {
                        for candidate in [Platform::Windows, Platform::Macos, Platform::Linux] { ui.selectable_value(&mut platform, candidate, candidate.label()); }
                    });
                });
                let mut changed = false;
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.checkbox(&mut self.features.preferences.free_only, "Free options only").changed();
                    changed |= ui.checkbox(&mut self.features.preferences.open_source_only, "Open source only").changed();
                });
                changed |= ui.add(egui::TextEdit::multiline(&mut self.features.preferences.priorities).hint_text("What matters to your workflow?").desired_rows(2).desired_width(f32::INFINITY).char_limit(1000)).changed();
                changed
            }).inner;
            self.features.target_platform = Some(platform);
            if changed || platform != original_platform { self.features.suggestions.clear(); self.features.suggestion_for = None; }
            ui.checkbox(&mut self.features.consent, "Send this app and my requirements to TypeSafe for comparison");
            if !self.features.configured { caption(ui, "Configure your TypeSafe key in Settings first.", p); }
            if ui.add_enabled(self.features.worker.is_some() && self.features.configured && self.features.consent && !self.features.ai_busy, egui::Button::new(if self.features.ai_busy { "Comparing alternatives…" } else { "Find alternatives" })).clicked()
                && self.features.send(FeatureCommand::Suggest { app: app.clone(), preferences: self.features.preferences.clone(), platform }) {
                self.features.ai_busy = true; self.features.suggestions.clear(); self.features.suggestion_for = Some(app.id.clone()); self.features.message = None;
            }
            if self.features.suggestion_for.as_deref() == Some(&app.id) {
                for suggestion in self.features.suggestions.clone() {
                    ui.separator();
                    ui.label(egui::RichText::new(&suggestion.name).strong());
                    ui.label(&suggestion.explanation);
                    for evidence in &suggestion.evidence { ui.hyperlink_to(&evidence.summary, &evidence.url); }
                    let accepted = self.features.state.accepted_alternatives.get(&app.id) == Some(&suggestion.product_id);
                    if ui.add_enabled(!accepted, egui::Button::new(if accepted { "Recommendation saved" } else { "Choose this alternative" })).clicked() {
                        self.features.send(FeatureCommand::Accept { app: app.id.clone(), candidate: suggestion.product_id.clone() });
                    }
                }
                if let Some(message) = &self.features.message { caption(ui, message, p); }
            }
            caption(ui, "Choosing a recommendation saves your preference. It does not install or remove software.", p);
        });
    }

    pub(super) fn program_update_control(&mut self, ui: &mut egui::Ui, app: &InstalledApp) {
        let winget_id = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .apps
                    .iter()
                    .position(|installed| installed.id == app.id)
            })
            .and_then(|index| self.matches.get(index))
            .and_then(|matched| match matched {
                MatchOutcome::Matched(product) => product.winget_id.clone(),
                _ => None,
            });
        let Some(winget_id) = winget_id else {
            return;
        };
        ui.add_space(8.0);
        if ui
            .add_enabled(
                cfg!(all(windows, target_arch = "x86_64")) && self.features.worker.is_some(),
                egui::Button::new("Check and update this installed app"),
            )
            .clicked()
            && let Some(device) = self.device.clone()
        {
            self.features.send(FeatureCommand::UpdateNow {
                device,
                program: ProgramPolicy {
                    app_id: app.id.clone(),
                    winget_id: winget_id.clone(),
                    display_name: app.name.clone(),
                    enabled: false,
                },
            });
        }
        if let Some(job) = self
            .features
            .state
            .jobs
            .iter()
            .rev()
            .find(|job| job.winget_id.as_ref() == Some(&winget_id))
        {
            ui.label(format!("Update status: {:?} · {}", job.status, job.outcome));
        }
    }
}
