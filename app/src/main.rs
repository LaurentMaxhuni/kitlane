#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shell;
mod theme;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 780.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../design-system/assets/icons/app-icon-128.png"
                ))
                .expect("bundled Kitlane icon"),
            )
            .with_min_inner_size([480.0, 360.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Kitlane",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, false);
            Ok(Box::new(shell::KitlaneApp::new(cc.egui_ctx.clone())))
        }),
    )
}
