use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

pub struct Palette {
    bg: Color32,
    surface: Color32,
    text: Color32,
    muted: Color32,
    border: Color32,
    action: Color32,
    action_text: Color32,
}

include!(concat!(env!("OUT_DIR"), "/palette.rs"));

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        FontData::from_static(include_bytes!(
            "../../design-system/assets/fonts/Inter-Variable.ttf"
        ))
        .into(),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["Inter".into()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["Inter".into()]);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let palette = if dark { &DARK } else { &LIGHT };
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.surface;
    visuals.override_text_color = Some(palette.text);
    visuals.widgets.noninteractive.fg_stroke.color = palette.muted;
    visuals.widgets.noninteractive.bg_stroke.color = palette.border;
    visuals.selection.bg_fill = palette.action;
    visuals.selection.stroke.color = palette.action_text;
    ctx.set_visuals(visuals);
}
