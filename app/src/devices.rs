use super::view::{caption, card};
use super::*;
use crate::theme::{self, Icon};
use egui::{RichText, Vec2};

pub(super) fn sync_age(scanned_at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |v| v.as_secs());
    if scanned_at > now {
        return "Timestamp ahead of this computer".into();
    }
    let elapsed = now.saturating_sub(scanned_at);
    match elapsed {
        0..60 => "Scanned less than a minute ago".into(),
        60..3600 => format!("Scanned {} minutes ago", elapsed / 60),
        3600..86400 => format!("Scanned {} hours ago", elapsed / 3600),
        _ => format!("Scanned {} days ago", elapsed / 86400),
    }
}

impl KitlaneApp {
    pub(super) fn devices_view(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        caption(ui, "YOUR WORKSPACE", p);
        ui.heading("Connected devices");
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Your paired computers, all in one place.").color(p.muted));
            if !self.paired_devices.is_empty() {
                if ui.button("Connect a device").clicked() {
                    self.open_pairing();
                }
                if ui.button("Sync libraries").clicked() {
                    self.pairing = Some(PairingStep::Done);
                }
            }
        });
        ui.add_space(24.0);
        if self.paired_devices.is_empty() {
            let height = (ui.ctx().content_rect().height() - 230.0).max(280.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space((height / 2.0 - 120.0).max(8.0));
                    theme::icon(ui, Icon::Devices, 48.0, p.info_text);
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new("Your workspace can go further")
                            .size(22.0)
                            .strong(),
                    );
                    caption(
                        ui,
                        "Connect another computer signed in to your Google account.",
                        p,
                    );
                    ui.add_space(18.0);
                    let response = ui.add(
                        egui::Button::image_and_text(
                            theme::icon_image(Icon::Plus, 16.0, p.action_text),
                            RichText::new("Connect a device").color(p.action_text),
                        )
                        .fill(p.action)
                        .min_size(Vec2::new(170.0, 42.0)),
                    );
                    theme::feedback(ui, &response);
                    if response.clicked() {
                        self.open_pairing();
                    }
                },
            );
            return;
        }
        caption(
            ui,
            format!(
                "{} paired device{}",
                self.paired_devices.len(),
                if self.paired_devices.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            p,
        );
        ui.add_space(16.0);
        for device in &self.paired_devices {
            let peer = self.peers.iter().find(|peer| peer.device.id == device.id);
            card(p).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    egui::Frame::new().fill(p.info_bg).corner_radius(12).inner_margin(12).show(ui, |ui| { theme::icon(ui, Icon::Devices, 32.0, p.info_text); });
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&device.name).size(18.0).strong());
                        caption(ui, format!("{} · {}", device.platform.label(),device.architecture), p);
                        if let Some(peer)=peer {caption(ui,format!("{} apps · {}",peer.apps.len(),sync_age(peer.scanned_at_unix_secs)),p);}
                        else {caption(ui,"Connected · inventory hasn't been shared yet",p);}
                    });
                });
                ui.add_space(12.0);
                if peer.is_some(){ui.push_id(&device.id, |ui| {
                    ui.collapsing("Shared apps & compatibility", |ui| {
                        caption(ui, "Compatibility with this computer. Review only; no apps are installed or updated.", p);
                        egui::ScrollArea::vertical().animated(!self.settings.reduced_motion).max_height(280.0).show(ui, |ui| {
                            for plan in self.plans.iter().filter(|plan| plan.source_devices.contains(&device.id)) {
                                ui.label(RichText::new(&plan.name).strong());
                                caption(ui, format!("{} · {}", plan.status.label(), plan.reason), p);
                                ui.add_space(8.0);
                            }
                            if self.plans.is_empty() { caption(ui, "Scan and sync this computer's Library to build compatibility proposals.", p); }
                        });
                    });
                });}
            });
            ui.add_space(16.0);
        }
    }
}
