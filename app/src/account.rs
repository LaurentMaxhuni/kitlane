use super::view::{brand, caption, primary};
use super::*;
use crate::theme::{self, Icon};
use egui::{Frame, RichText, Vec2};

impl KitlaneApp {
    pub(super) fn clear_account(&mut self) {
        self.account = None;
        self.peers.clear();
        self.paired_devices.clear();
        self.plans.clear();
        self.pairing = None;
        self.pairing_transition = None;
        self.sync = SyncUi::default();
    }
    pub(super) fn sign_in_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(24)).show(ui,|ui|{
            egui::ScrollArea::vertical().animated(!self.settings.reduced_motion).show(ui,|ui|{
                ui.add_space(((ui.ctx().content_rect().height()-48.0)/2.0-250.0).max(24.0));
                ui.horizontal(|ui|{
                    ui.add_space(((ui.available_width()-500.0)/2.0).max(0.0));
                    ui.vertical(|ui|{
                        ui.set_width(ui.available_width().min(500.0));
                        brand(ui,self.dark,false);ui.add_space(36.0);
                        ui.heading(RichText::new("Your apps. Your devices.").size(32.0));ui.add_space(12.0);
                        ui.label(RichText::new("Sign in with Google to bring your computers together.").color(p.muted));ui.add_space(28.0);
                        Frame::new().fill(p.surface).stroke(egui::Stroke::new(1.0,p.border)).corner_radius(14).inner_margin(24).show(ui,|ui|{
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui|{theme::icon(ui,Icon::Shield,22.0,p.info_text);ui.label("One account for your connected devices");});
                            caption(ui,"Use the same Google account on each computer. Pair devices with a short code.",p);ui.add_space(22.0);
                            if self.auth_busy{
                                ui.horizontal(|ui|{theme::activity(ui);ui.label(if self.signing_in{"Finish signing in in your browser"}else{"Checking your saved account…"});});
                                if self.signing_in&&ui.button("Cancel sign-in").clicked()&&let Some(worker)=&self.worker{worker.cancel_sign_in();}
                            }else{
                                let ready=self.worker.is_some()&&self.device.is_some();
                                let response=ui.add_enabled(ready,primary("Continue with Google",p,ready).min_size(Vec2::new(ui.available_width(),48.0)));
                                theme::feedback(ui,&response);
                                if response.clicked()&&let Some(worker)=&self.worker{
                                    match worker.try_send(Command::SignIn){Ok(())=>{self.auth_busy=true;self.signing_in=true;self.error=None;},Err(_)=>{self.error=Some("Sign-in could not start. Try again.".into());}}
                                }
                            }
                            if let Some(error)=&self.error{ui.add_space(16.0);ui.colored_label(p.danger_text,error);}
                        });
                        ui.add_space(20.0);caption(ui,"Inventory stays on this computer until you choose Sync.",p);
                    });
                });
            });
        });
    }
}
