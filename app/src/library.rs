use super::view::{caption, card};
use super::*;
use crate::theme;
use egui::RichText;

impl KitlaneApp {
    pub(super) fn library_details(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .apps
                .iter()
                .position(|app| Some(app.id.as_str()) == self.selected_app.as_deref())
        }) else {
            return;
        };
        let app = &self.snapshot.as_ref().unwrap().apps[index];
        let p = theme::palette(self.dark);
        let mut close = false;
        card(p).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&app.name).size(18.0).strong());
                if ui.button("Close details").clicked() { close = true; }
            });
            ui.label(format!("{} · {}", app.publisher.as_deref().unwrap_or("Publisher unknown"),app.version.as_deref().unwrap_or("Version unknown")));
            ui.push_id(&app.id, |ui| {
                egui::CollapsingHeader::new("App details & availability").default_open(true).show(ui, |ui| {
                    caption(ui, format!("Inventory source: {:?} · scope: {:?}",app.source,app.scope),p);
                    if let Some(native) = &app.native_id { ui.label(format!("Native identifier: {native}")); }
                    if let Some(origin) = &app.origin { ui.label(format!("Package origin: {origin}")); }
                    if let Some(MatchOutcome::Matched(product)) = self.matches.get(index) {
                        ui.horizontal_wrapped(|ui| {
                            for support in &product.platforms { ui.label(format!("{}: {}",support.platform.label(),support.availability.label())); }
                        });
                        caption(ui,"Catalog associations use metadata; executable identity and target compatibility still need verification.",p);
                    } else { caption(ui,"This app has no reviewed catalog match. Availability stays unknown.",p); }
                    if ui.button("Copy app information").clicked() { ui.ctx().copy_text(format!("{}\nPublisher: {}\nVersion: {}",app.name,app.publisher.as_deref().unwrap_or("Unknown"),app.version.as_deref().unwrap_or("Unknown"))); }
                });
            });
        });
        ui.add_space(16.0);
        if close {
            self.selected_app = None;
        }
    }
}
