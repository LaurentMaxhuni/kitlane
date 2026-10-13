use std::sync::{Arc, OnceLock};

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Response, Stroke, TextStyle,
};

pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub action: Color32,
    pub action_text: Color32,
    pub focus: Color32,
    pub selected: Color32,
    pub info_text: Color32,
    pub info_bg: Color32,
    pub success_text: Color32,
    pub warning_text: Color32,
    pub danger_text: Color32,
    pub danger_bg: Color32,
}

include!(concat!(env!("OUT_DIR"), "/palette.rs"));

pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "Inter",
            include_bytes!("../../design-system/assets/fonts/Inter-Variable.ttf").as_slice(),
        ),
        (
            "Sora",
            include_bytes!("../../design-system/assets/fonts/Sora-Variable.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
    }
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    let mut display_fonts = vec!["Sora".into()];
    display_fonts.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts
        .families
        .insert(FontFamily::Name("Sora".into()), display_fonts);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = palette(dark);
    let mode = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let mut style = (*ctx.style_of(mode)).clone();
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(28.0, FontFamily::Name("Sora".into())),
        ),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 10.0);
    style.spacing.interact_size = egui::vec2(42.0, 42.0);
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 10.0;
    style.spacing.icon_spacing = 8.0;
    style.spacing.window_margin = egui::Margin::same(24);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.animation_time = 0.16;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.floating_width = 3.0;
    style.spacing.scroll.floating_allocated_width = 3.0;
    style.spacing.scroll.dormant_handle_opacity = 0.25;
    style.spacing.scroll.active_handle_opacity = 0.5;
    style.spacing.scroll.interact_handle_opacity = 0.85;
    style.spacing.scroll.dormant_background_opacity = 0.0;
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = None;
    v.weak_text_color = Some(p.muted);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = egui::CornerRadius::same(12);
    v.menu_corner_radius = egui::CornerRadius::same(8);
    v.faint_bg_color = p.bg;
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.code_bg_color = p.bg;
    v.hyperlink_color = p.info_text;
    v.warn_fg_color = p.warning_text;
    v.error_fg_color = p.danger_text;
    v.selection.bg_fill = p.selected;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.text_cursor.stroke = Stroke::new(2.0, p.focus);
    v.text_cursor.blink = false;
    v.image_loading_spinners = false;
    v.disabled_alpha = 0.6;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.bg_fill = p.surface;
        widget.weak_bg_fill = p.surface;
        widget.bg_stroke = Stroke::new(1.0, p.border);
        widget.fg_stroke = Stroke::new(1.75, p.text);
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.expansion = 0.0;
    }
    v.widgets.noninteractive.weak_bg_fill = p.bg;
    v.widgets.hovered.bg_fill = p.selected;
    v.widgets.hovered.weak_bg_fill = p.selected;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.focus);
    v.widgets.active.bg_fill = p.selected;
    v.widgets.active.weak_bg_fill = p.selected;
    v.widgets.active.bg_stroke = Stroke::new(2.0, p.focus);
    v.widgets.open.bg_fill = p.surface_raised;
    v.widgets.open.weak_bg_fill = p.surface_raised;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.focus);
    style.visuals = v;
    ctx.set_style_of(mode, style);
    ctx.set_theme(mode);
}

pub fn motion(ctx: &egui::Context, reduced: bool) {
    let duration = if reduced { 0.0 } else { 0.16 };
    if ctx.global_style().animation_time != duration {
        ctx.global_style_mut(|style| style.animation_time = duration);
        if reduced {
            ctx.clear_animations();
        }
    }
}

pub fn activity(ui: &mut egui::Ui) {
    if ui.style().animation_time == 0.0 {
        icon(ui, Icon::Refresh, 18.0, ui.visuals().weak_text_color());
    } else {
        ui.spinner();
    }
}

pub fn feedback(ui: &egui::Ui, response: &Response) {
    let amount = ui.ctx().animate_bool(
        response.id.with("feedback"),
        response.hovered() || response.has_focus(),
    );
    if amount > 0.0 && response.enabled() {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            8,
            Stroke::new(
                1.0,
                ui.visuals()
                    .selection
                    .stroke
                    .color
                    .gamma_multiply(amount * 0.6),
            ),
            egui::StrokeKind::Outside,
        );
    }
}

#[derive(Clone, Copy)]
pub enum Icon {
    Library,
    Devices,
    Settings,
    Search,
    Refresh,
    Copy,
    Monitor,
    Shield,
    Sun,
    Moon,
    Alert,
    Plus,
}

pub fn icon(ui: &mut egui::Ui, kind: Icon, size: f32, color: Color32) -> Response {
    ui.add(icon_image(kind, size, color))
}

pub fn icon_image(kind: Icon, size: f32, color: Color32) -> egui::Image<'static> {
    // SVG tint multiplies source pixels; normalize Lucide's currentColor to white once.
    macro_rules! asset {
        ($name:literal) => {{
            static BYTES: OnceLock<Arc<[u8]>> = OnceLock::new();
            (
                concat!("bytes://kitlane/icons/", $name, ".svg"),
                BYTES
                    .get_or_init(|| {
                        include_str!(concat!(
                            "../../design-system/assets/ui-icons/",
                            $name,
                            ".svg"
                        ))
                        .replace("currentColor", "#FFFFFF")
                        .into_bytes()
                        .into()
                    })
                    .clone(),
            )
        }};
    }
    let (uri, bytes) = match kind {
        Icon::Library => asset!("package"),
        Icon::Devices => asset!("laptop"),
        Icon::Settings => asset!("settings"),
        Icon::Search => asset!("search"),
        Icon::Refresh => asset!("refresh-cw"),
        Icon::Copy => asset!("copy"),
        Icon::Monitor => asset!("monitor"),
        Icon::Shield => asset!("shield-check"),
        Icon::Sun => asset!("sun"),
        Icon::Moon => asset!("moon"),
        Icon::Alert => asset!("circle-alert"),
        Icon::Plus => asset!("plus"),
    };
    egui::Image::from_bytes(uri, bytes)
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(color)
        .show_loading_spinner(false)
}
