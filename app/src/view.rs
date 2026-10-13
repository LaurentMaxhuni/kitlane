use super::*;
use crate::theme::{self, Icon, Palette};
use egui::{Color32, Frame, Margin, RichText, Stroke, Vec2};
use kitlane_core::{
    catalog::Availability,
    model::{InstallScope, InventorySource},
};

fn card(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(20)
}

fn caption(ui: &mut egui::Ui, text: impl Into<String>, p: &Palette) {
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

fn primary(text: &str, p: &Palette, enabled: bool) -> egui::Button<'static> {
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

fn brand(ui: &mut egui::Ui, dark: bool, compact: bool) {
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
            egui::ScrollArea::vertical().show(ui, |ui| {
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
                    else { ui.spinner(); }
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
                                    if ui.add_sized([140.0, 64.0], egui::Button::new(label).selected(self.dark == dark)).clicked() {
                                        self.dark = dark;
                                        theme::apply(ui.ctx(), dark);
                                    }
                                }
                            });
                            ui.add_space(12.0);
                            caption(ui, "You can change appearance later in Settings.", p);
                        }
                        2 => {
                            let label = ui.label("What should we call this device?");
                            ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(&mut self.setup_name).hint_text("For example, Studio workstation").desired_width(f32::INFINITY).char_limit(128)).labelled_by(label.id);
                            ui.add_space(20.0);
                            ui.checkbox(&mut self.settings.scan_on_startup, "Scan installed apps now and on future launches");
                            caption(ui, "Turn this off to scan manually from Library. Scanning does not install, update, or remove apps.", p);
                        }
                        _ => {
                            ui.label(format!("Device: {}", self.setup_name.trim()));
                            ui.label(format!("Appearance: {}", if self.dark { "Dark" } else { "Light" }));
                            ui.label(format!("Inventory: {}", if self.settings.scan_on_startup { "Scan on launch" } else { "Manual scans" }));
                            ui.add_space(20.0);
                            ui.checkbox(&mut self.setup_pair, "Open Devices to pair another computer after setup");
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
                    if self.setup_busy { ui.spinner(); ui.label("Saving your choices…"); }
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
            if ui.add_sized(size, button).clicked() {
                self.page = page;
            }
        }
    }

    pub(super) fn content(&mut self, ui: &mut egui::Ui) {
        self.receive_events(ui.ctx());
        if self.onboarding || (self.worker.is_some() && self.device.is_none()) {
            self.setup_view(ui);
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
                    brand(ui, self.dark, true);
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
                    brand(ui, self.dark, false);
                    ui.add_space(40.0);
                    caption(ui, "WORKSPACE", p);
                    ui.add_space(12.0);
                    self.navigation(ui, false);
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        caption(ui, "Kitlane  ·  0.1", p);
                        ui.add_space(12.0);
                        Frame::new()
                            .fill(p.bg)
                            .corner_radius(10)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    theme::icon(ui, Icon::Shield, 18.0, p.muted);
                                    ui.label(
                                        RichText::new(if self.sync.enrolled {
                                            "Enrolled locally"
                                        } else {
                                            "Local workspace"
                                        })
                                        .size(12.0),
                                    );
                                });
                                caption(ui, "You choose what gets shared.", p);
                            });
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
                            .id_salt("library-page")
                            .show(ui, |ui| self.library_view(ui, height))
                            .inner
                    }
                    Page::Devices => {
                        egui::ScrollArea::vertical()
                            .id_salt("devices-page")
                            .show(ui, |ui| self.devices_view(ui))
                            .inner
                    }
                    Page::Settings => {
                        egui::ScrollArea::vertical()
                            .id_salt("settings-page")
                            .show(ui, |ui| self.settings_view(ui))
                            .inner
                    }
                }
            });
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
                    ui.spinner();
                    ui.label(RichText::new("Scanning…").size(12.0).color(p.muted));
                } else {
                    caption(ui, format!("{} apps shown", self.rows.len()), p);
                }
            });
        });
        ui.add_space(12.0);
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
                                            Frame::new()
                                                .fill(p.bg)
                                                .corner_radius(10)
                                                .inner_margin(10)
                                                .show(ui, |ui| {
                                                    let letter = app
                                                        .name
                                                        .chars()
                                                        .next()
                                                        .unwrap_or('?')
                                                        .to_uppercase()
                                                        .to_string();
                                                    ui.add_sized(
                                                        [22.0, 22.0],
                                                        egui::Label::new(
                                                            RichText::new(letter)
                                                                .size(18.0)
                                                                .color(p.text),
                                                        ),
                                                    );
                                                });
                                            ui.allocate_ui_with_layout(
                                                Vec2::new(ui.available_width(), 42.0),
                                                egui::Layout::top_down(egui::Align::LEFT),
                                                |ui| {
                                                    ui.set_min_height(42.0);
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&app.name)
                                                                .size(14.0)
                                                                .strong(),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(&app.name);
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

    fn devices_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        caption(ui, "YOUR WORKSPACE", p);
        ui.heading("Your computers");
        ui.label(
            RichText::new("A clear view of each setup, with you in control of sharing.")
                .color(p.muted),
        );
        ui.add_space(24.0);
        if let Some(device) = &self.device {
            card(p).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    Frame::new()
                        .fill(p.selected)
                        .corner_radius(12)
                        .inner_margin(16)
                        .show(ui, |ui| {
                            theme::icon(ui, Icon::Monitor, 32.0, p.info_text);
                        });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&device.name).size(18.0));
                        caption(
                            ui,
                            format!("{} · {}", device.platform.label(), device.architecture),
                            p,
                        );
                        badge(ui, "This computer", p.info_text, p.info_bg);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_min_width(ui.available_width());
                        let button = egui::Button::image_and_text(
                            theme::icon_image(Icon::Copy, 16.0, p.text),
                            "Copy identity",
                        );
                        if ui
                            .add(button)
                            .on_hover_text("Copy this device's pairing metadata.")
                            .clicked()
                            && let Ok(identity) = serde_json::to_string(device)
                        {
                            ui.ctx().copy_text(identity);
                        }
                    });
                });
                ui.add_space(12.0);
                ui.collapsing("Device identity", |ui| {
                    ui.monospace(&device.id);
                });
            });
            ui.add_space(16.0);
        }
        for peer in &self.peers {
            card(p).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Devices, 26.0, p.muted);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&peer.device.name).size(16.0));
                        caption(
                            ui,
                            format!(
                                "{} · {} apps",
                                peer.device.platform.label(),
                                peer.apps.len()
                            ),
                            p,
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_min_width(ui.available_width());
                        badge(ui, "Shared inventory", p.info_text, p.info_bg);
                    });
                });
            });
            ui.add_space(16.0);
        }
        card(p).show(ui, |ui| { ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Connect your setup").size(18.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.set_min_width(ui.available_width()); badge(ui, if self.sync.enrolled { "Enrolled" } else { "Local only" }, p.info_text, p.info_bg); });
            });
            caption(ui, "Connect to your local Kitlane sync service. Remote-device transport is not available yet.", p);
            ui.add_space(16.0);
            ui.add_enabled_ui(!self.sync.busy, |ui| {
                ui.label("Service address");
                if ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(&mut self.sync.endpoint).hint_text("http://127.0.0.1:8787")).changed() {
                    self.sync.enrolled = false; self.sync.consent = false; self.sync.pairing_token.zeroize(); self.peers.clear(); self.plans.clear(); self.sync.message = None;
                }
                ui.add_space(8.0);
                caption(ui, "Enrolled devices under this owner can read your app names, publishers, versions, and device metadata.", p);
                ui.checkbox(&mut self.sync.consent, "I agree to share inventory with this service and its enrolled devices");
                ui.add_space(12.0);
                ui.label("Pairing token");
                ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(&mut self.sync.pairing_token).password(true).char_limit(64).hint_text("Paste the token issued by your service owner"));
                ui.add_space(16.0);
                let ready = self.device.is_some() && self.worker.is_some();
                let enroll_ready = ready && self.sync.consent && !self.sync.enrolled && self.sync.pairing_token.len() == 64;
                let sync_ready = ready && self.sync.consent && self.sync.enrolled && self.snapshot.is_some() && !self.scanning;
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(enroll_ready, primary("Enroll this device", p, enroll_ready)).clicked() {
                        let pairing_token = zeroize::Zeroizing::new(std::mem::take(&mut self.sync.pairing_token));
                        self.send_sync(Command::Enroll { endpoint: self.sync.endpoint.clone(), pairing_token });
                    }
                    if ui.add_enabled(sync_ready, primary("Sync now", p, sync_ready)).clicked()
                        && let Some(snapshot) = self.snapshot.clone() { self.send_sync(Command::Sync { endpoint: self.sync.endpoint.clone(), snapshot: Box::new(snapshot) }); }
                    if ui.add_enabled(ready, egui::Button::new("Check saved enrollment")).clicked() { self.send_sync(Command::CheckEnrollment { endpoint: self.sync.endpoint.clone() }); }
                });
                ui.add_space(12.0);
                ui.collapsing("Credential recovery & removal", |ui| {
                    caption(ui, "For lost enrollment credentials, ask the service owner for a recovery pairing token. Forgetting a local credential does not revoke it on the service.", p);
                    if ui.add_enabled(ready, egui::Button::new(RichText::new("Forget credential").color(p.danger_text))).clicked() { self.send_sync(Command::Disconnect { endpoint: self.sync.endpoint.clone() }); }
                });
            });
            if self.sync.busy { ui.horizontal(|ui| { ui.spinner(); caption(ui, "Working with the sync service…", p); }); }
            if let Some(message) = &self.sync.message { ui.add_space(12.0); caption(ui, message, p); }
        });
        if !self.plans.is_empty() {
            ui.add_space(24.0);
            ui.label(RichText::new("Compatibility proposals").size(18.0));
            caption(
                ui,
                "Review only. No programs are installed, updated, or removed.",
                p,
            );
            ui.add_space(12.0);
            card(p).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("compatibility-plans")
                    .max_height(320.0)
                    .show_rows(ui, 64.0, self.plans.len(), |ui, range| {
                        for index in range {
                            let plan = &self.plans[index];
                            ui.allocate_ui(Vec2::new(ui.available_width(), 64.0), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&plan.name).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(&plan.name);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            badge(ui, plan.status.label(), p.muted, p.bg);
                                        },
                                    );
                                });
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&plan.reason).size(12.0).color(p.muted),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&plan.reason);
                            });
                        }
                    });
            });
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        caption(ui, "YOUR WORKSPACE", p);
        ui.heading("Make it yours");
        ui.label(RichText::new("A few preferences for a calmer workspace.").color(p.muted));
        ui.add_space(24.0);
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Appearance").size(18.0));
            caption(ui, "Choose how Kitlane looks on this computer.", p);
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                for (dark, label, icon) in [(false, "Light", Icon::Sun), (true, "Dark", Icon::Moon)]
                {
                    let selected = self.dark == dark;
                    let button = egui::Button::image_and_text(
                        theme::icon_image(icon, 20.0, if selected { p.info_text } else { p.muted }),
                        label,
                    )
                    .fill(if selected { p.selected } else { p.bg })
                    .stroke(Stroke::new(
                        if selected { 2.0 } else { 1.0 },
                        if selected { p.info_text } else { p.border },
                    ));
                    if ui
                        .add_enabled(
                            self.worker.is_none() || self.device.is_some(),
                            button.min_size(Vec2::new(160.0, 72.0)),
                        )
                        .clicked()
                        && !selected
                    {
                        self.dark = dark;
                        theme::apply(ui.ctx(), dark);
                        if let Some(worker) = &self.worker
                            && let Err(error) =
                                worker.try_send(Command::SaveSettings(AppSettings {
                                    dark_theme: dark,
                                    ..self.settings.clone()
                                }))
                        {
                            self.error = Some(format!("Appearance could not be saved: {error}"));
                        }
                        self.settings.dark_theme = dark;
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
            let mut scan = self.settings.scan_on_startup;
            if ui
                .checkbox(&mut scan, "Scan installed apps on launch")
                .changed()
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
