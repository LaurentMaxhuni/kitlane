use super::view::{caption, primary};
use super::*;
use crate::theme::{self, Icon};
use egui::{RichText, Vec2};

impl KitlaneApp {
    pub(super) fn open_pairing(&mut self) {
        if self.account.is_none() {
            return;
        }
        self.pairing = Some(PairingStep::Service);
        self.sync.message = None;
    }
    fn close_pairing(&mut self) {
        self.pairing = None;
        self.pairing_transition = None;
        self.sync.code_input.zeroize();
        self.sync.generated = None;
    }
    fn pairing_ready(&self) -> bool {
        self.account.is_some()
            && self.device.is_some()
            && self.worker.is_some()
            && !self.sync.busy
            && kitlane_account_client::valid_code(&self.sync.code_input)
    }
    pub(super) fn pairing_wizard(&mut self, ctx: &egui::Context) {
        let Some(step) = self.pairing else { return };
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
        let response=egui::Modal::new(egui::Id::new("device-pairing-wizard")).frame(egui::Frame::new().fill(p.surface).stroke(egui::Stroke::new(1.0,p.border)).corner_radius(16).inner_margin(if ctx.content_rect().width()<600.0{24}else{32})).show(ctx,|ui|{
            ui.multiply_opacity(opacity);ui.set_width((ctx.content_rect().width()-96.0).clamp(240.0,500.0));
            egui::ScrollArea::vertical().animated(!self.settings.reduced_motion).max_height((ctx.content_rect().height()-220.0).max(100.0)).show(ui,|ui|{
                ui.horizontal(|ui|{theme::icon(ui,Icon::Devices,24.0,p.info_text);ui.heading("Connect your devices");});ui.add_space(12.0);
                if let Some(account)=&self.account {caption(ui,format!("Signed in as {}",account.email),p);}
                let index=match step{PairingStep::Service=>1,PairingStep::Consent=>2,PairingStep::Review=>3,PairingStep::Done=>4};
                ui.add_space(12.0);ui.add(egui::ProgressBar::new(index as f32/4.0).desired_height(4.0).fill(p.action));ui.add_space(24.0);
                ui.add_enabled_ui(!self.sync.busy,|ui|match step{
                    PairingStep::Service=>{
                        ui.label(RichText::new("Where would you like to start?").size(18.0));
                        caption(ui,"Sign in to the same Google account on both computers. Then create a code on one and enter it on the other.",p);ui.add_space(20.0);
                        let ready=self.worker.is_some();
                        if ui.add_enabled(ready,primary("Create a pairing code",p,ready).min_size(Vec2::new(ui.available_width(),48.0))).clicked(){self.sync.creating_code=true;self.sync.generated=None;self.pairing=Some(PairingStep::Consent);self.send_sync(Command::GenerateCode);}
                        ui.add_space(12.0);
                        if ui.add_sized([ui.available_width(),48.0],egui::Button::new("Enter a code from another device")).clicked(){self.sync.creating_code=false;self.sync.code_input.zeroize();self.pairing=Some(PairingStep::Consent);}
                    }
                    PairingStep::Consent if self.sync.creating_code=>{
                        ui.label(RichText::new("Enter this code on your other computer").strong());ui.add_space(16.0);
                        if let Some(code)=&self.sync.generated{
                            let remaining=code.expires_at.saturating_sub(kitlane_account_client::now());
                            ui.label(RichText::new(format!("{}  {}",&code.code[..4],&code.code[4..])).monospace().size(32.0).color(if remaining>0{p.text}else{p.muted}));
                            caption(ui,if remaining>0{format!("Expires in {}:{:02}",remaining/60,remaining%60)}else{"This code expired. Create a new one.".into()},p);
                            if remaining>0{ctx.request_repaint_after(std::time::Duration::from_secs(1));}
                            ui.add_space(16.0);
                            if ui.add_enabled(remaining>0,egui::Button::image_and_text(theme::icon_image(Icon::Copy,16.0,p.text),"Copy code")).clicked(){ctx.copy_text(code.code.clone());self.sync.message=Some("Code copied.".into());}
                        }
                        ui.add_space(12.0);
                        if ui.add_enabled(self.worker.is_some(),egui::Button::new("Create a new code")).clicked(){self.sync.generated=None;self.send_sync(Command::GenerateCode);}
                        if ui.add_enabled(self.worker.is_some(),egui::Button::new("Check connection")).clicked(){self.send_sync(Command::RefreshDevices);}
                        caption(ui,"Pairing connects these devices. It does not share their app libraries.",p);
                    }
                    PairingStep::Consent=>{
                        let label=ui.label("Pairing code");ui.add_space(8.0);
                        if theme::input(ui,&mut self.sync.code_input,"ABCD2345",false,9).labelled_by(label.id).changed(){self.sync.code_input=self.sync.code_input.trim().replace([' ','-'],"").to_ascii_uppercase();}
                        ui.add_space(14.0);caption(ui,"Codes have eight characters and expire after five minutes. Both devices must use the Google account shown above.",p);
                    }
                    PairingStep::Review=>{
                        ui.label(RichText::new("Ready to connect").size(18.0));
                        caption(ui,"Your code will be used once to connect this computer to the other device on your account.",p);ui.add_space(20.0);
                        ui.label(RichText::new(&self.sync.code_input).monospace().size(24.0));ui.add_space(12.0);
                        caption(ui,"You can choose whether to share inventory after connecting.",p);
                    }
                    PairingStep::Done=>{
                        theme::icon(ui,Icon::Shield,32.0,p.success_text);ui.heading("Your devices are connected");
                        caption(ui,"Pairing is complete. Inventory is shared only when you choose Sync.",p);ui.add_space(20.0);
                        theme::toggle(ui,&mut self.sync.consent,"Share my app inventory with paired devices on this account");
                        if self.snapshot.is_none(){ui.add_space(12.0);if ui.button("Scan this computer").clicked(){self.scan();}}
                        let ready=self.worker.is_some()&&self.account.is_some()&&self.snapshot.is_some()&&self.sync.consent;
                        ui.add_space(16.0);if ui.add_enabled(ready,primary("Sync now",p,ready)).clicked()&&let Some(snapshot)=self.snapshot.clone(){self.send_sync(Command::Sync{snapshot:Box::new(snapshot)});}
                    }
                });
                if self.sync.busy{ui.add_space(16.0);ui.horizontal(|ui|{theme::activity(ui);ui.label("Connecting securely…");});}
                if let Some(message)=&self.sync.message{ui.add_space(12.0);ui.label(message);}
            });
            ui.add_space(24.0);
            ui.horizontal(|ui|{
                if ui.add_enabled(!self.sync.busy,egui::Button::new(if step==PairingStep::Done{"Done"}else{"Cancel"})).clicked(){self.close_pairing();}
                if step!=PairingStep::Service&&step!=PairingStep::Done&&ui.add_enabled(!self.sync.busy,egui::Button::new("Back")).clicked(){self.pairing=Some(PairingStep::Service);}
                match step{
                    PairingStep::Consent if !self.sync.creating_code=>{let ready=self.pairing_ready();if ui.add_enabled(ready,primary("Continue",p,ready)).clicked(){self.pairing=Some(PairingStep::Review);}}
                    PairingStep::Review=>{let ready=self.pairing_ready();if ui.add_enabled(ready,primary("Connect device",p,ready)).clicked(){let code=zeroize::Zeroizing::new(std::mem::take(&mut self.sync.code_input));self.send_sync(Command::PairCode{code});}}
                    _=>{}
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
                    assert!(app.pairing.is_some());
                    assert!(!app.sync.busy);
                }
            }
        }
    }
    #[test]
    fn pairing_requires_verified_account_and_valid_code_then_discards_code_on_close() {
        let (worker, _, _) = worker::test_worker();
        let mut app = KitlaneApp::preview(0, false, "normal");
        app.worker = Some(worker);
        app.sync.code_input = "ABCD2345".into();
        assert!(app.pairing_ready());
        app.sync.code_input = "ABCD01IO".into();
        assert!(!app.pairing_ready());
        app.sync.code_input = "ABCD2345".into();
        app.account = None;
        assert!(!app.pairing_ready());
        app.close_pairing();
        assert!(app.sync.code_input.is_empty());
    }
}
