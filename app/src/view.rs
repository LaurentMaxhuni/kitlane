use super::*;
use crate::theme::{self, Icon, Palette};
use egui::{Color32, Frame, Margin, RichText, Stroke, Vec2};
use kitlane_core::{
    catalog::Availability,
    model::{InstallScope, InventorySource},
};

pub(super) fn card(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(20)
}

pub(super) fn caption(ui: &mut egui::Ui, text: impl Into<String>, p: &Palette) {
    ui.label(RichText::new(text).size(12.0).color(p.muted));
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32, fill: Color32) {
    Frame::new()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color));
        });
}

pub(super) fn primary(text: &str, p: &Palette, enabled: bool) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(if enabled {
        p.action_text
    } else {
        p.muted
    }))
    .fill(if enabled { p.action } else { p.surface_raised })
    .stroke(if enabled {
        Stroke::NONE
    } else {
        Stroke::new(1.0, p.border)
    })
    .min_size(Vec2::new(120.0, 42.0))
}

pub(super) fn brand(ui: &mut egui::Ui, dark: bool, compact: bool) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Image::from_bytes(
                "bytes://kitlane-app-icon",
                include_bytes!("../../design-system/assets/icons/app-icon-128.png"),
            )
            .fit_to_exact_size(Vec2::splat(if compact { 28.0 } else { 34.0 })),
        );
        let (uri, bytes): (&str, &[u8]) = if dark {
            (
                "bytes://kitlane-wordmark-dark",
                include_bytes!("../../design-system/assets/logos/kitlane-wordmark-reversed.png"),
            )
        } else {
            (
                "bytes://kitlane-wordmark-light",
                include_bytes!("../../design-system/assets/logos/kitlane-wordmark.png"),
            )
        };
        ui.add(egui::Image::from_bytes(uri, bytes).fit_to_exact_size(Vec2::new(106.0, 30.0)));
    });
}

impl KitlaneApp {
    fn save_setup_step(&mut self, step: u8, completed: bool) {
        if self.account.is_none() {
            return;
        }
        let Some(worker) = &self.worker else {
            return;
        };
        let settings = AppSettings {
            dark_theme: self.dark,
            onboarding_step: step,
            onboarding_completed: completed,
            ..self.settings.clone()
        };
        match worker.try_send(Command::SaveSetup {
            name: self.setup_name.clone(),
            settings,
        }) {
            Ok(()) => {
                self.setup_busy = true;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Setup could not be saved: {error}. Try again."))
            }
        }
    }

    fn setup_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(24)).show(ui, |ui| {
            egui::ScrollArea::vertical().animated(!self.settings.reduced_motion).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 640.0) / 2.0).max(0.0));
                    ui.vertical(|ui| {
                let width = ui.available_width().min(640.0);
                ui.set_width(width);
                brand(ui, self.dark, false);
                ui.add_space(24.0);
                let step = self.settings.onboarding_step;
                caption(ui, format!("SET UP YOUR WORKSPACE  ·  STEP {} OF 4", step + 1), p);
                ui.add_space(8.0);
                ui.add(egui::ProgressBar::new((step + 1) as f32 / 4.0).fill(p.action).desired_height(6.0).desired_width(ui.available_width()));
                ui.add_space(24.0);
                if self.device.is_none() {
                    ui.heading("Getting your workspace ready");
                    if let Some(error) = &self.error { ui.colored_label(p.danger_text, error); }
                    else { theme::activity(ui); }
                    return;
                }
                let titles = ["A little setup. A clearer workspace.", "Make Kitlane feel like yours", "Tell us about this computer", "Ready when you are"];
                ui.label(RichText::new(titles[usize::from(step)]).size(28.0).strong());
                ui.add_space(12.0);
                card(p).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add_enabled_ui(!self.setup_busy, |ui| match step {
                        0 => {
                            ui.label("Bring your installed apps into one library and see where they work across your devices.");
                            ui.add_space(20.0);
                            theme::icon(ui, Icon::Shield, 28.0, p.info_text);
                            ui.label(RichText::new("Your library starts here, on this computer.").strong());
                            ui.add_space(8.0);
                            caption(ui, "Scanning reads installed-app metadata. Sharing requires optional pairing, explicit consent, and Sync now.", p);
                        }
                        1 => {
                            ui.label("Which appearance works best for you?");
                            ui.add_space(16.0);
                            ui.horizontal_wrapped(|ui| {
                                for (dark, label) in [(false, "Light"), (true, "Dark")] {
                                    if theme::choice(ui, self.dark == dark, label).clicked() {
                                        self.dark = dark;
                                        theme::apply(ui.ctx(), dark);
                                    }
                                }
                            });
                            ui.add_space(12.0);
                            caption(ui, "You can change appearance later in Settings.", p);
                            if theme::toggle(ui, &mut self.settings.reduced_motion, "Reduced motion").changed() { theme::motion(ui.ctx(), self.settings.reduced_motion); }
                        }
                        2 => {
                            let label = ui.label("What should we call this device?");
                            theme::input(ui, &mut self.setup_name, "For example, Studio workstation", false, 128).labelled_by(label.id);
                            ui.add_space(20.0);
                            theme::toggle(ui, &mut self.settings.scan_on_startup, "Scan installed apps now and on future launches");
                            caption(ui, "Turn this off to scan manually from Library. Scanning does not install, update, or remove apps.", p);
                        }
                        _ => {
                            ui.label(format!("Device: {}", self.setup_name.trim()));
                            ui.label(format!("Appearance: {}", if self.dark { "Dark" } else { "Light" }));
                            ui.label(format!("Inventory: {}", if self.settings.scan_on_startup { "Scan on launch" } else { "Manual scans" }));
                            ui.add_space(20.0);
                            theme::toggle(ui, &mut self.setup_pair, "Open Devices to pair another computer after setup");
                            caption(ui, "Optional. Pairing needs a local service and an owner-issued grant. No inventory is shared during setup.", p);
                        }
                    });
                });
                ui.add_space(16.0);
                if let Some(error) = &self.error { ui.colored_label(p.danger_text, error); }
                let valid = self.device.as_ref().is_some_and(|device| {
                    let mut draft = device.clone(); draft.name = self.setup_name.trim().to_owned(); draft.validate().is_ok()
                });
                if step == 2 && !valid { ui.colored_label(p.danger_text, "Use a device name of 1–128 bytes without control characters."); }
                ui.horizontal_wrapped(|ui| {
                    if step > 0 && ui.add_enabled(!self.setup_busy, egui::Button::new("Back").min_size(Vec2::new(100.0, 42.0))).clicked() { self.save_setup_step(step - 1, false); }
                    let enabled = !self.setup_busy && valid;
                    if ui.add_enabled(enabled, primary(if step == 3 { "Open Kitlane" } else { "Continue" }, p, enabled)).clicked() { self.save_setup_step((step + 1).min(3), step == 3); }
                    if self.setup_busy { theme::activity(ui); ui.label("Saving your choices…"); }
                });
                    });
                });
            });
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui, compact: bool) {
        let p = theme::palette(self.dark);
        for (page, label, icon) in [
            (Page::Library, "Library", Icon::Library),
            (Page::Devices, "Devices", Icon::Devices),
            (Page::Settings, "Settings", Icon::Settings),
        ] {
            let selected = self.page == page;
            let button = egui::Button::image_and_text(
                theme::icon_image(icon, 18.0, if selected { p.info_text } else { p.muted }),
                RichText::new(label).color(if selected { p.info_text } else { p.text }),
            )
            .fill(if selected {
                p.selected
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE)
            .corner_radius(8);
            let size = if compact {
                Vec2::new(108.0, 38.0)
            } else {
                Vec2::new(ui.available_width(), 44.0)
            };
            let response = ui.add_sized(size, button);
            let hover = ui.ctx().animate_bool(
                response.id.with("accent"),
                response.hovered() || response.has_focus() || selected,
            );
            if hover > 0.0 {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        response.rect.left_top() + Vec2::new(2.0, 10.0),
                        Vec2::new(3.0, response.rect.height() - 20.0),
                    ),
                    2,
                    p.info_text.gamma_multiply(hover),
                );
            }
            if response.clicked() {
                self.page = page;
            }
        }
    }

    pub(super) fn content(&mut self, ui: &mut egui::Ui) {
        self.receive_events(ui.ctx());
        theme::motion(ui.ctx(), self.settings.reduced_motion);
        self.window_chrome(ui);
        if self.account.is_none() {
            self.sign_in_view(ui);
            return;
        }
        let key = (self.page, self.onboarding, self.settings.onboarding_step);
        if self.transition_key != Some(key) {
            self.transition_key = Some(key);
            ui.ctx()
                .animate_bool_with_time(egui::Id::new("page-enter"), false, 0.0);
        }
        let enter = if self.settings.reduced_motion {
            1.0
        } else {
            ui.ctx().animate_bool(egui::Id::new("page-enter"), true)
        };
        ui.multiply_opacity(0.45 + enter * 0.55);
        if self.onboarding || (self.worker.is_some() && self.device.is_none()) {
            self.setup_view(ui);
            self.pairing_wizard(ui.ctx());
            return;
        }
        let p = theme::palette(self.dark);
        let compact = ui.available_width() < 860.0;
        if compact {
            egui::Panel::top("compact-navigation")
                .frame(
                    Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(20, 12)),
                )
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| self.navigation(ui, true));
                });
        } else {
            egui::Panel::left("navigation")
                .exact_size(216.0)
                .resizable(false)
                .frame(
                    Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(20, 28)),
                )
                .show(ui, |ui| {
                    ui.label(RichText::new("Workspace").size(16.0).strong());
                    ui.add_space(40.0);
                    caption(ui, "EXPLORE", p);
                    ui.add_space(12.0);
                    self.navigation(ui, false);
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        caption(ui, "Kitlane  ·  0.1", p);
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(if compact { 20 } else { 32 }),
            )
            .show(ui, |ui| {
                if let Some(error) = &self.error {
                    Frame::new()
                        .fill(p.danger_bg)
                        .corner_radius(8)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                theme::icon(ui, Icon::Alert, 18.0, p.danger_text);
                                ui.add(
                                    egui::Label::new(RichText::new(error).color(p.danger_text))
                                        .wrap(),
                                );
                            });
                        });
                    ui.add_space(16.0);
                }
                match self.page {
                    Page::Library => {
                        let height = ui.available_height();
                        egui::ScrollArea::vertical()
                            .animated(!self.settings.reduced_motion)
                            .id_salt("library-page")
                            .show(ui, |ui| self.library_view(ui, height))
                            .inner
                    }
                    Page::Devices => {
                        egui::ScrollArea::vertical()
                            .animated(!self.settings.reduced_motion)
                            .id_salt("devices-page")
                            .show(ui, |ui| self.devices_view(ui))
                            .inner
                    }
                    Page::Settings => {
                        egui::ScrollArea::vertical()
                            .animated(!self.settings.reduced_motion)
                            .id_salt("settings-page")
                            .show(ui, |ui| self.settings_view(ui))
                            .inner
                    }
                }
            });
        self.pairing_wizard(ui.ctx());
    }

    fn library_view(&mut self, ui: &mut egui::Ui, height: f32) {
        let p = theme::palette(self.dark);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                caption(ui, "YOUR WORKSPACE", p);
                ui.heading("Your library");
                ui.label(
                    RichText::new("Your tools. One place to keep them together.").color(p.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_width(ui.available_width());
                let enabled = !self.scanning && self.worker.is_some();
                let color = if enabled { p.action_text } else { p.muted };
                let button = egui::Button::image_and_text(
                    theme::icon_image(Icon::Refresh, 16.0, color),
                    RichText::new("Refresh library").color(color),
                )
                .fill(if enabled { p.action } else { p.surface_raised })
                .stroke(if enabled {
                    Stroke::NONE
                } else {
                    Stroke::new(1.0, p.border)
                })
                .min_size(Vec2::new(148.0, 42.0));
                if ui
                    .add_enabled(!self.scanning && self.worker.is_some(), button)
                    .clicked()
                {
                    self.scan();
                }
            });
        });
        ui.add_space(28.0);
        let count = self
            .snapshot
            .as_ref()
            .map_or(0, |snapshot| snapshot.apps.len());
        let known = self
            .matches
            .iter()
            .filter(|m| matches!(m, MatchOutcome::Matched(_)))
            .count();
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                Frame::new()
                    .fill(p.selected)
                    .corner_radius(10)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        theme::icon(ui, Icon::Monitor, 26.0, p.info_text);
                    });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let reserve = if ui.available_width() > 500.0 {
                        280.0
                    } else {
                        120.0
                    };
                    ui.set_max_width((ui.available_width() - reserve).max(80.0));
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                self.device
                                    .as_ref()
                                    .map_or("This computer", |device| &device.name),
                            )
                            .size(16.0)
                            .strong(),
                        )
                        .truncate(),
                    );
                    caption(
                        ui,
                        self.device.as_ref().map_or(
                            "Reading your local setup…".to_owned(),
                            |device| {
                                format!("{} · {}", device.platform.label(), device.architecture)
                            },
                        ),
                        p,
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.allocate_ui_with_layout(
                        Vec2::new(96.0, 60.0),
                        egui::Layout::top_down(egui::Align::RIGHT),
                        |ui| {
                            ui.set_min_size(Vec2::new(96.0, 60.0));
                            ui.label(RichText::new(count.to_string()).size(26.0).strong());
                            caption(ui, "installed apps", p);
                        },
                    );
                    if ui.available_width() > 100.0 {
                        ui.add_space(24.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(110.0, 60.0),
                            egui::Layout::top_down(egui::Align::RIGHT),
                            |ui| {
                                ui.set_min_size(Vec2::new(110.0, 60.0));
                                ui.label(RichText::new(known.to_string()).size(26.0).strong());
                                caption(ui, "catalog matches", p);
                            },
                        );
                    }
                });
            });
        });
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(12, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon(ui, Icon::Search, 18.0, p.muted);
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .frame(Frame::NONE)
                                    .desired_width((ui.available_width() - 130.0).max(150.0))
                                    .hint_text("Search apps or publishers"),
                            )
                            .changed()
                        {
                            self.filter_rows();
                        }
                    });
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_width(ui.available_width());
                if self.scanning {
                    theme::activity(ui);
                    ui.label(RichText::new("Scanning…").size(12.0).color(p.muted));
                } else {
                    caption(ui, format!("{} apps shown", self.rows.len()), p);
                }
            });
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            for (filter, label) in [
                (LibraryFilter::All, "All apps"),
                (LibraryFilter::Known, "Catalog matches"),
                (LibraryFilter::Unknown, "Unmatched"),
            ] {
                if ui
                    .selectable_label(self.library_filter == filter, label)
                    .clicked()
                {
                    self.library_filter = filter;
                    self.filter_rows();
                }
            }
            if !self.query.is_empty() && ui.button("Clear search").clicked() {
                self.query.clear();
                self.filter_rows();
            }
        });
        ui.add_space(12.0);
        self.library_details(ui);
        if self.cached {
            caption(
                ui,
                "Showing saved inventory while this computer is checked.",
                p,
            );
            ui.add_space(8.0);
        }
        let Some(snapshot) = &self.snapshot else {
            self.empty_library(
                ui,
                p,
                if self.scanning {
                    "Getting your tools together"
                } else {
                    "Your library starts here"
                },
                if self.scanning {
                    "Reading installed applications on this computer…"
                } else {
                    "Refresh your library to see the applications on this computer."
                },
            );
            return;
        };
        if self.rows.is_empty() {
            self.empty_library(
                ui,
                p,
                if self.query.is_empty() {
                    "No applications found"
                } else {
                    "No matching applications"
                },
                if self.query.is_empty() {
                    "Check scan coverage below, then refresh to try again."
                } else {
                    "Try a different app name or publisher."
                },
            );
        } else {
            let row_width = ui.available_width().max(740.0);
            let height = (height - 440.0).max(120.0);
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(16, 0))
                .show(ui, |ui| {
                    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                    egui::ScrollArea::horizontal()
                        .animated(!self.settings.reduced_motion)
                        .id_salt("library-horizontal")
                        .scroll_bar_visibility(if row_width > ui.available_width() + 32.0 {
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible
                        } else {
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                        })
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_width(row_width - 32.0);
                            egui_extras::TableBuilder::new(ui)
                                .id_salt("library-table")
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(egui_extras::Column::remainder().clip(true))
                                .column(egui_extras::Column::exact(84.0).clip(true))
                                .columns(egui_extras::Column::exact(90.0).clip(true), 3)
                                .max_scroll_height(height)
                                .auto_shrink([false, false])
                                .header(44.0, |mut row| {
                                    for label in
                                        ["APPLICATION", "VERSION", "WINDOWS", "MACOS", "LINUX"]
                                    {
                                        row.col(|ui| {
                                            ui.label(
                                                RichText::new(label).size(10.0).color(p.muted),
                                            );
                                        });
                                    }
                                })
                                .body(|body| {
                                    body.rows(72.0, self.rows.len(), |mut row| {
                                        let index = self.rows[row.index()];
                                        let app = &snapshot.apps[index];
                                        row.col(|ui| {
                                            logos::show(
                                                ui,
                                                app,
                                                self.matches.get(index),
                                                p,
                                                self.local_icons.get(&app.id),
                                            );
                                            ui.allocate_ui_with_layout(
                                                Vec2::new(ui.available_width(), 42.0),
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    ui.set_min_height(42.0);
                                                    let response = ui
                                                        .add(
                                                            egui::Label::new(
                                                                RichText::new(&app.name)
                                                                    .size(14.0)
                                                                    .strong(),
                                                            )
                                                            .truncate()
                                                            .sense(egui::Sense::click()),
                                                        )
                                                        .on_hover_text(format!(
                                                            "{} · Click for app details",
                                                            app.name
                                                        ));
                                                    response.widget_info(|| {
                                                        egui::WidgetInfo::labeled(
                                                            egui::WidgetType::Button,
                                                            true,
                                                            format!(
                                                                "Show details for {}",
                                                                app.name
                                                            ),
                                                        )
                                                    });
                                                    theme::feedback(ui, &response);
                                                    if response.clicked() {
                                                        self.selected_app =
                                                            if self.selected_app.as_deref()
                                                                == Some(&app.id)
                                                            {
                                                                None
                                                            } else {
                                                                Some(app.id.clone())
                                                            };
                                                    }
                                                    let scope = match app.scope {
                                                        InstallScope::User => "User",
                                                        InstallScope::Machine => "Machine",
                                                        InstallScope::Unknown => "Unknown scope",
                                                    };
                                                    let source = match app.source {
                                                        InventorySource::WindowsRegistry => {
                                                            format!(
                                                                "{} · {scope}",
                                                                app.publisher
                                                                    .as_deref()
                                                                    .unwrap_or("Publisher unknown")
                                                            )
                                                        }
                                                        InventorySource::MacosBundle => format!(
                                                            "macOS bundle · {scope} · {}",
                                                            app.native_id
                                                                .as_deref()
                                                                .unwrap_or("Unknown bundle ID")
                                                        ),
                                                        InventorySource::Flatpak => format!(
                                                            "Flatpak · {scope} · {} (unverified)",
                                                            app.origin
                                                                .as_deref()
                                                                .unwrap_or("Unknown origin")
                                                        ),
                                                        InventorySource::Manual => format!(
                                                            "Manual · {scope} · {}",
                                                            app.publisher
                                                                .as_deref()
                                                                .unwrap_or("Publisher unknown")
                                                        ),
                                                    };
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&source)
                                                                .size(11.0)
                                                                .color(p.muted),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(format!(
                                                        "{source}\nObserved identifier: {}",
                                                        app.native_id
                                                            .as_deref()
                                                            .unwrap_or("Unknown")
                                                    ));
                                                },
                                            );
                                        });
                                        row.col(|ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(
                                                        app.version.as_deref().unwrap_or("Unknown"),
                                                    )
                                                    .size(12.0)
                                                    .color(p.muted),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(
                                                app.version.as_deref().unwrap_or("Version unknown"),
                                            );
                                        });
                                        for platform in
                                            [Platform::Windows, Platform::Macos, Platform::Linux]
                                        {
                                            row.col(|ui| {
                                                if let Some(MatchOutcome::Matched(product)) =
                                                    self.matches.get(index)
                                                {
                                                    let support = product.support(platform);
                                                    let color = if support.availability
                                                        == Availability::Available
                                                    {
                                                        p.success_text
                                                    } else {
                                                        p.muted
                                                    };
                                                    if let Some(evidence) = support.evidence.first()
                                                    {
                                                        ui.add(
                                                            egui::Hyperlink::from_label_and_url(
                                                                RichText::new(
                                                                    support.availability.label(),
                                                                )
                                                                .size(11.0)
                                                                .color(color),
                                                                &evidence.url,
                                                            ),
                                                        )
                                                        .on_hover_text(&support.compatibility_note);
                                                    } else {
                                                        ui.label(
                                                            RichText::new("Unknown")
                                                                .size(11.0)
                                                                .color(p.muted),
                                                        );
                                                    }
                                                } else {
                                                    ui.label(
                                                        RichText::new("Unknown")
                                                            .size(11.0)
                                                            .color(p.muted),
                                                    )
                                                    .on_hover_text(
                                                        "No reviewed catalog association.",
                                                    );
                                                }
                                            });
                                        }
                                    })
                                });
                        });
                });
        }
        ui.add_space(12.0);
        ui.collapsing("About availability & scan coverage", |ui| {
            caption(ui, "OS availability uses reviewed vendor evidence. CPU and OS-version compatibility are not checked yet.", p);
            caption(ui, "Catalog matches use observed metadata and reviewed identifiers; installation still requires verification.", p);
            for warning in &snapshot.warnings { caption(ui, warning, p); }
        });
    }

    fn empty_library(&self, ui: &mut egui::Ui, p: &Palette, heading: &str, detail: &str) {
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(180.0);
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                theme::icon(ui, Icon::Library, 32.0, p.muted);
                ui.add_space(12.0);
                ui.label(RichText::new(heading).size(18.0));
                ui.label(RichText::new(detail).color(p.muted));
            });
        });
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        caption(ui, "YOUR WORKSPACE", p);
        ui.heading("Make it yours");
        if let Some(account) = self.account.clone() {
            ui.add_space(20.0);
            card(p).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("Google account").size(18.0));
                ui.label(&account.email);
                caption(ui, "Pair computers signed in to this account.", p);
                ui.add_space(12.0);
                if ui
                    .add_enabled(self.worker.is_some(), egui::Button::new("Sign out"))
                    .clicked()
                    && let Some(worker) = &self.worker
                {
                    match worker.try_send(Command::SignOut) {
                        Ok(()) => {
                            self.clear_account();
                            self.auth_busy = true;
                        }
                        Err(_) => self.error = Some("Sign-out could not start. Try again.".into()),
                    }
                }
            });
            ui.add_space(16.0);
        }
        ui.label(RichText::new("A few preferences for a calmer workspace.").color(p.muted));
        ui.add_space(24.0);
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Appearance").size(18.0));
            caption(ui, "Choose how Kitlane looks on this computer.", p);
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                for (dark, label) in [(false, "Light"), (true, "Dark")] {
                    let selected = self.dark == dark;
                    if theme::choice(ui, selected, label).clicked() && !selected {
                        self.save_preferences(
                            AppSettings {
                                dark_theme: dark,
                                ..self.settings.clone()
                            },
                            ui.ctx(),
                        );
                    }
                }
            });
        });
        ui.add_space(16.0);
        card(p).show(ui, |ui| { ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| { theme::icon(ui, Icon::Shield, 24.0, p.info_text); ui.label(RichText::new("You're in control").size(18.0)); });
            ui.add_space(8.0);
            ui.label("Inventory sharing happens only when you choose Sync now. Device credentials stay in your operating system's credential store.");
        });
        ui.add_space(16.0);
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Library preferences").size(18.0));
            let mut reduced = self.settings.reduced_motion;
            if theme::toggle(ui, &mut reduced, "Reduced motion").changed() { self.save_preferences(AppSettings { reduced_motion: reduced, ..self.settings.clone() }, ui.ctx()); }
            caption(ui, "Use instant transitions and static loading indicators. Your choice is saved for this computer.", p);
            let mut scan = self.settings.scan_on_startup;
            if theme::toggle(ui, &mut scan, "Scan installed apps on launch").changed()
            {
                let settings = AppSettings {
                    scan_on_startup: scan,
                    ..self.settings.clone()
                };
                if let Some(worker) = &self.worker {
                    match worker.try_send(Command::SaveSettings(settings.clone())) {
                        Ok(()) => self.settings = settings,
                        Err(error) => {
                            self.error = Some(format!("Preferences could not be saved: {error}"))
                        }
                    }
                }
            }
            if ui
                .add_enabled(
                    !self.setup_busy && self.device.is_some(),
                    egui::Button::new("Run setup again"),
                )
                .clicked()
            {
                self.setup_name = self.device.as_ref().unwrap().name.clone();
                self.save_setup_step(0, false);
            }
        });
        ui.add_space(16.0);
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Automation").size(18.0));
            caption(
                ui,
                "Installation, updates, and AI alternatives are not enabled yet.",
                p,
            );
            ui.add_space(12.0);
            for label in [
                "Install compatible counterparts",
                "Keep applications up to date",
                "Suggest alternatives",
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_min_width(ui.available_width());
                        badge(ui, "Not enabled", p.muted, p.bg);
                    });
                });
                ui.add_space(8.0);
            }
        });
    }
}
