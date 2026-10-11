use eframe::egui;

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
}

impl KitlaneApp {
    fn content(&mut self, ui: &mut egui::Ui) {
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
        ui.add_space(24.0);
        match self.page {
            Page::Library => {
                ui.heading("Your app library");
                ui.label("No apps added yet.");
                ui.add_space(8.0);
                ui.label("Software discovery and installation will be connected here.");
            }
            Page::Devices => {
                ui.heading("Your devices");
                ui.label("No connected devices.");
                ui.add_space(8.0);
                ui.label("Device pairing and sync will be connected here.");
            }
            Page::Settings => {
                ui.heading("Appearance");
                if ui.checkbox(&mut self.dark, "Dark theme").changed() {
                    crate::theme::apply(ui.ctx(), self.dark);
                }
                ui.label("Appearance resets when you close the app.");
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
                let mut app = KitlaneApp { page, dark };
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
}
