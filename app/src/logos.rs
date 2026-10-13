use crate::theme::Palette;
use eframe::egui::{self, RichText};
use kitlane_core::{catalog::MatchOutcome, model::InstalledApp};

/// Fixed vendor associations only. Unknown metadata uses a local initial tile.
fn asset(
    app: &InstalledApp,
    matched: Option<&MatchOutcome>,
) -> Option<(&'static str, &'static [u8], &'static str)> {
    let id = match matched {
        Some(MatchOutcome::Matched(product)) => product.id.as_str(),
        _ if app.name == "Blender" && app.publisher.as_deref() == Some("Blender Foundation") => {
            "blender"
        }
        _ => return None,
    };
    match id {
        "visual-studio-code" => Some((
            "vscode.png",
            include_bytes!("../assets/programs/visual-studio-code.png"),
            "https://code.visualstudio.com/",
        )),
        "vlc-media-player" => Some((
            "vlc.svg",
            include_bytes!("../assets/programs/vlc.svg"),
            "https://www.videolan.org/vlc/",
        )),
        "notepad-plus-plus" => Some((
            "notepad.svg",
            include_bytes!("../assets/programs/notepad-plus-plus.svg"),
            "https://notepad-plus-plus.org/",
        )),
        "blender" => Some((
            "blender.svg",
            include_bytes!("../assets/programs/blender.svg"),
            "https://www.blender.org/",
        )),
        _ => None,
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &InstalledApp,
    matched: Option<&MatchOutcome>,
    p: &Palette,
) {
    if let Some((id, bytes, url)) = asset(app, matched) {
        let response = ui
            .add(
                egui::Image::from_bytes(format!("bytes://program/{id}"), bytes)
                    .max_size(egui::vec2(34.0, 34.0))
                    .maintain_aspect_ratio(true)
                    .sense(egui::Sense::click()),
            )
            .on_hover_text("Open the official program website");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Link,
                true,
                format!("{} official website", app.name),
            )
        });
        if response.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    } else {
        egui::Frame::new()
            .fill(p.selected)
            .corner_radius(10)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.add_sized(
                    [22.0, 22.0],
                    egui::Label::new(
                        RichText::new(
                            app.name
                                .chars()
                                .next()
                                .unwrap_or('?')
                                .to_uppercase()
                                .to_string(),
                        )
                        .size(18.0)
                        .color(p.info_text),
                    ),
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitlane_core::{
        catalog::Catalog,
        model::{InstallScope, InventorySource},
    };

    #[test]
    fn unknown_names_and_unreviewed_publishers_do_not_get_vendor_logos() {
        let mut app = InstalledApp {
            id: "test".into(),
            name: "VLC media player".into(),
            publisher: Some("Someone else".into()),
            version: None,
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: None,
            origin: None,
        };
        let catalog = Catalog::bundled().unwrap();
        assert!(asset(&app, Some(&catalog.match_app(&app))).is_none());
        app.publisher = Some("VideoLAN".into());
        assert_eq!(
            asset(&app, Some(&catalog.match_app(&app))).unwrap().0,
            "vlc.svg"
        );
        app.name = "Blender".into();
        assert!(asset(&app, None).is_none());
    }
}
