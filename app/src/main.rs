#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shell;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([480.0, 360.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Kitlane",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, false);
            Ok(Box::new(shell::KitlaneApp::default()))
        }),
    )
}
