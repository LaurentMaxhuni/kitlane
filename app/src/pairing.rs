use super::view::{caption, primary};
use super::*;
use crate::theme::{self, Icon};
use egui::{RichText, Vec2};

impl KitlaneApp {
    pub(super) fn open_pairing(&mut self) {
        self.pairing = Some(if self.sync.enrolled {
            PairingStep::Done
        } else {
            PairingStep::Service
        });
        self.sync.message = None;
    }

    fn close_pairing(&mut self) {
        self.pairing = None;
        self.pairing_transition = None;
        self.sync.pairing_token.zeroize();
    }

    fn pairing_service_valid(&self) -> bool {
        let Some(device) = &self.device else {
            return false;
        };
        device.validate().is_ok()
            && kitlane_sync_client::validate_endpoint(&self.sync.endpoint).is_ok()
    }

    fn pairing_ready(&self) -> bool {
        self.pairing_service_valid()
            && self.worker.is_some()
            && self.sync.consent
            && !self.sync.busy
            && kitlane_sync_client::valid_token(&self.sync.pairing_token)
    }

    pub(super) fn pairing_wizard(&mut self, ctx: &egui::Context) {
        let Some(step) = self.pairing else {
            return;
        };
        if self.pairing_transition != Some(step) {
            self.pairing_transition = Some(step);
            ctx.animate_bool_with_time(egui::Id::new("pairing-enter"), false, 0.0);
        }
        let opacity = if self.settings.reduced_motion {
            1.0
        } else {
            0.45 + 0.55 * ctx.animate_bool(egui::Id::new("pairing-enter"), true)
        };
        let p = theme::palette(self.dark);
        let response = egui::Modal::new(egui::Id::new("device-pairing-wizard")).show(ctx, |ui| {
            ui.multiply_opacity(opacity);
            ui.set_width((ctx.content_rect().width() - 64.0).clamp(260.0, 480.0));
            egui::ScrollArea::vertical().animated(!self.settings.reduced_motion).max_height((ctx.content_rect().height() - 180.0).max(120.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Devices, 24.0, p.info_text);
                    ui.heading("Connect your devices");
                });
                ui.add_space(12.0);
                let index = match step { PairingStep::Service => 1, PairingStep::Consent => 2, PairingStep::Review => 3, PairingStep::Done => 4 };
                caption(ui, format!("STEP {index} OF 4"), p);
                ui.add(egui::ProgressBar::new(index as f32 / 4.0).desired_height(4.0).fill(p.action));
                ui.add_space(18.0);
                ui.add_enabled_ui(!self.sync.busy, |ui| match step {
                    PairingStep::Service => {
                        ui.label("Start with your Kitlane service");
                        caption(ui, "Connect this computer to the local sync service to discover other enrolled devices. Remote transport is not available yet.", p);
                        ui.add_space(12.0);
                        let label = ui.label("Service address");
                        if ui.add_sized([ui.available_width(),42.0], egui::TextEdit::singleline(&mut self.sync.endpoint).hint_text("http://127.0.0.1:8787")).labelled_by(label.id).changed() {
                            self.sync.enrolled = false; self.sync.consent = false; self.sync.pairing_token.zeroize(); self.peers.clear(); self.plans.clear(); self.sync.message = None;
                        }
                        ui.add_space(12.0);
                        if let Some(device) = &self.device {
                            caption(ui, format!("Pairing {}", device.name), p);
                            if ui.add(egui::Button::image_and_text(theme::icon_image(Icon::Copy, 16.0, p.text), "Copy device identity for the service owner")).clicked() {
                                match serde_json::to_string(device) { Ok(value) => ui.ctx().copy_text(value), Err(error) => self.sync.message = Some(format!("Cannot copy identity: {error}")) }
                            }
                        }
                        if !self.pairing_service_valid() { ui.colored_label(p.danger_text, "Use http://127.0.0.1 with a valid port."); }
                        if ui.add_enabled(self.worker.is_some() && self.pairing_service_valid(), egui::Button::new("Check saved connection")).clicked() { self.send_sync(Command::CheckEnrollment { endpoint: self.sync.endpoint.clone() }); }
                    }
                    PairingStep::Consent => {
                        ui.label("Choose what you share");
                        caption(ui, "Enrolled devices under this owner can read your app names, publishers, versions, and device metadata. Inventory stays local until you click Sync now.", p);
                        ui.add_space(12.0);
                        ui.checkbox(&mut self.sync.consent, "I agree to share inventory with this service and its enrolled devices");
                        ui.add_space(16.0);
                        let label = ui.label("Pairing token from the service owner");
                        ui.add_sized([ui.available_width(),42.0], egui::TextEdit::singleline(&mut self.sync.pairing_token).password(true).char_limit(64).hint_text("Paste your 64-character grant")).labelled_by(label.id);
                        caption(ui, "For lost credentials, request a recovery grant for this device. Grants are single-use; an uncertain response needs the owner's help.", p);
                    }
                    PairingStep::Review => {
                        ui.label(RichText::new("Ready to connect").strong());
                        ui.label(format!("Service: {}", self.sync.endpoint));
                        if let Some(device) = &self.device { ui.label(format!("Device: {}", device.name)); }
                        caption(ui, "Connecting saves your credential in the operating system's credential store. It does not upload inventory.", p);
                    }
                    PairingStep::Done => {
                        ui.label(RichText::new("This computer is paired").strong().color(p.success_text));
                        caption(ui, "Sync to retrieve other devices and share your current inventory. The list shows paired devices with shared snapshots; it does not indicate live online status.", p);
                        ui.add_space(12.0);
                        ui.checkbox(&mut self.sync.consent, "I agree to share inventory with this service and its enrolled devices");
                        let ready = self.worker.is_some() && self.sync.consent && self.snapshot.is_some() && !self.scanning;
                        if ui.add_enabled(ready, primary("Sync now", p, ready)).clicked() && let Some(snapshot) = self.snapshot.clone() { self.send_sync(Command::Sync { endpoint: self.sync.endpoint.clone(), snapshot: Box::new(snapshot) }); }
                        if self.snapshot.is_none() { caption(ui, "Scan your Library first, then return here to sync.", p); }
                        ui.add_space(12.0);
                        ui.collapsing("Credential recovery & removal", |ui| {
                            caption(ui, "For lost credentials, request a recovery pairing token. Forgetting a local credential does not revoke server access.", p);
                            if ui.add_enabled(self.worker.is_some(), egui::Button::new(RichText::new("Forget credential").color(p.danger_text))).clicked() { self.send_sync(Command::Disconnect { endpoint: self.sync.endpoint.clone() }); self.pairing = Some(PairingStep::Service); }
                        });
                    }
                });
                if let Some(message) = &self.sync.message { ui.add_space(12.0); ui.label(message); }
                if self.sync.busy { ui.horizontal(|ui| { theme::activity(ui); ui.label("Connecting to your service…"); }); }
            });
                ui.add_space(20.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.sync.busy, egui::Button::new(if step == PairingStep::Done { "Done" } else { "Cancel" }).min_size(Vec2::new(90.0,42.0))).clicked() { self.close_pairing(); }
                    if matches!(step, PairingStep::Consent | PairingStep::Review) && ui.add_enabled(!self.sync.busy, egui::Button::new("Back")).clicked() { self.pairing = Some(if step == PairingStep::Review { PairingStep::Consent } else { PairingStep::Service }); }
                    let ready = !self.sync.busy && match step { PairingStep::Service => self.pairing_service_valid(), PairingStep::Consent | PairingStep::Review => self.pairing_ready(), PairingStep::Done => false };
                    if step != PairingStep::Done && ui.add_enabled(ready, primary(if step == PairingStep::Review { "Connect device" } else { "Continue" }, p, ready)).clicked() {
                        self.sync.message = None;
                        match step {
                            PairingStep::Service => self.pairing = Some(PairingStep::Consent),
                            PairingStep::Consent => self.pairing = Some(PairingStep::Review),
                            PairingStep::Review => {
                                let pairing_token = zeroize::Zeroizing::new(std::mem::take(&mut self.sync.pairing_token));
                                self.send_sync(Command::Enroll { endpoint: self.sync.endpoint.clone(), pairing_token });
                            }
                            PairingStep::Done => {}
                        }
                    }
                });
        });
        if response.should_close() && !self.sync.busy {
            self.close_pairing();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_wizard_steps_render_in_both_themes_and_window_sizes() {
        for dark in [false, true] {
            for page in 7..=10 {
                for width in [480.0, 900.0, 1440.0] {
                    let ctx = egui::Context::default();
                    crate::theme::install_fonts(&ctx);
                    crate::theme::apply(&ctx, dark);
                    egui_extras::install_image_loaders(&ctx);
                    let mut app = KitlaneApp::preview(page, dark, "normal");
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| app.content(ui));
                    output.textures_delta.clear();
                    assert!(!output.shapes.is_empty());
                    assert!(app.pairing.is_some());
                    assert!(!app.sync.busy);
                }
            }
        }
    }

    #[test]
    fn pairing_requires_consent_valid_address_and_valid_grant() {
        let (worker, _, _) = worker::test_worker();
        let mut app = KitlaneApp::preview(0, false, "normal");
        app.worker = Some(worker);
        app.sync.pairing_token = "a".repeat(64);
        assert!(!app.pairing_ready());
        app.sync.consent = true;
        assert!(app.pairing_ready());
        for address in [
            "https://example.com",
            "http://127.0.0.1:0",
            "http://127.0.0.1:8787/path",
        ] {
            app.sync.endpoint = address.into();
            assert!(!app.pairing_ready());
        }
        app.sync.endpoint = "http://127.0.0.1:8787".into();
        app.sync.pairing_token = "z".repeat(64);
        assert!(!app.pairing_ready());
        app.close_pairing();
        assert!(app.sync.pairing_token.is_empty());
    }
}
