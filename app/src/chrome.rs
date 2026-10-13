use super::view::{brand, caption};
use super::*;
use crate::theme::{self, Icon};
use egui::{Color32, Frame, Margin, RichText, Sense, Vec2, ViewportCommand};

impl KitlaneApp {
    pub(super) fn save_preferences(&mut self, settings: AppSettings, ctx: &egui::Context) {
        if let Some(worker) = &self.worker
            && let Err(error) = worker.try_send(Command::SaveSettings(settings.clone()))
        {
            self.error = Some(format!("Preferences could not be saved: {error}"));
            return;
        }
        self.dark = settings.dark_theme;
        self.settings = settings;
        theme::apply(ctx, self.dark);
        theme::motion(ctx, self.settings.reduced_motion);
    }

    pub(super) fn window_chrome(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(self.dark);
        egui::Panel::top("window-titlebar")
            .exact_size(48.0)
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let brand_response = ui
                        .horizontal(|ui| brand(ui, self.dark, true))
                        .response
                        .interact(Sense::CLICK | Sense::DRAG);
                    if brand_response.is_pointer_button_down_on()
                        && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                    {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    if ui.available_width() > 540.0 {
                        ui.add_space(12.0);
                        caption(ui, "YOUR PERSONAL WORKSPACE", p);
                    }
                    let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (glyph, label, command) in [
                            ("×", "Close window", ViewportCommand::Close),
                            (
                                if max { "❐" } else { "□" },
                                if max {
                                    "Restore window"
                                } else {
                                    "Maximize window"
                                },
                                ViewportCommand::Maximized(!max),
                            ),
                            ("−", "Minimize window", ViewportCommand::Minimized(true)),
                        ] {
                            let response = ui
                                .add_sized(
                                    [34.0, 34.0],
                                    egui::Button::new(RichText::new(glyph).size(20.0))
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE),
                                )
                                .on_hover_text(label);
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                            });
                            let hover = ui.ctx().animate_bool(
                                response.id.with("hover"),
                                response.hovered() || response.has_focus(),
                            );
                            if hover > 0.0 {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    7,
                                    egui::Stroke::new(
                                        1.0,
                                        (if label == "Close window" {
                                            p.danger_text
                                        } else {
                                            p.focus
                                        })
                                        .gamma_multiply(hover),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if response.clicked() {
                                ui.ctx().send_viewport_cmd(command);
                            }
                        }
                        if ui.available_width() > 40.0 {
                            let response = ui
                                .add_sized(
                                    [34.0, 34.0],
                                    egui::Button::image(theme::icon_image(
                                        if self.dark { Icon::Sun } else { Icon::Moon },
                                        17.0,
                                        p.muted,
                                    ))
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE),
                                )
                                .on_hover_text("Switch appearance");
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    "Switch appearance",
                                )
                            });
                            if response.clicked() {
                                self.save_preferences(
                                    AppSettings {
                                        dark_theme: !self.dark,
                                        ..self.settings.clone()
                                    },
                                    ui.ctx(),
                                );
                            }
                        }
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width().max(0.0), 34.0),
                            Sense::CLICK | Sense::DRAG,
                        );
                        if response.double_clicked() {
                            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
                        } else if response.is_pointer_button_down_on()
                            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                        {
                            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                        ui.painter().line_segment(
                            [rect.left_bottom(), rect.right_bottom()],
                            egui::Stroke::new(0.0, p.border),
                        );
                    });
                });
            });
        // Borderless windows need resize hit targets on all edges and corners.
        let rect = ui.ctx().content_rect();
        if !ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
            let regions = [
                (
                    egui::Rect::from_min_max(rect.min, rect.min + Vec2::splat(6.0)),
                    egui::ResizeDirection::NorthWest,
                    egui::CursorIcon::ResizeNwSe,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.right() - 6.0, rect.top()),
                        egui::pos2(rect.right(), rect.top() + 6.0),
                    ),
                    egui::ResizeDirection::NorthEast,
                    egui::CursorIcon::ResizeNeSw,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 6.0),
                        egui::pos2(rect.left() + 6.0, rect.bottom()),
                    ),
                    egui::ResizeDirection::SouthWest,
                    egui::CursorIcon::ResizeNeSw,
                ),
                (
                    egui::Rect::from_min_max(rect.max - Vec2::splat(6.0), rect.max),
                    egui::ResizeDirection::SouthEast,
                    egui::CursorIcon::ResizeNwSe,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 6.0, rect.top()),
                        egui::pos2(rect.right() - 6.0, rect.top() + 4.0),
                    ),
                    egui::ResizeDirection::North,
                    egui::CursorIcon::ResizeVertical,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 6.0, rect.bottom() - 4.0),
                        egui::pos2(rect.right() - 6.0, rect.bottom()),
                    ),
                    egui::ResizeDirection::South,
                    egui::CursorIcon::ResizeVertical,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.top() + 6.0),
                        egui::pos2(rect.left() + 4.0, rect.bottom() - 6.0),
                    ),
                    egui::ResizeDirection::West,
                    egui::CursorIcon::ResizeHorizontal,
                ),
                (
                    egui::Rect::from_min_max(
                        egui::pos2(rect.right() - 4.0, rect.top() + 6.0),
                        egui::pos2(rect.right(), rect.bottom() - 6.0),
                    ),
                    egui::ResizeDirection::East,
                    egui::CursorIcon::ResizeHorizontal,
                ),
            ];
            if let Some(position) = ui.ctx().pointer_hover_pos() {
                for (region, direction, cursor) in regions {
                    if region.contains(position) {
                        ui.ctx().set_cursor_icon(cursor);
                        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                            ui.ctx()
                                .send_viewport_cmd(ViewportCommand::BeginResize(direction));
                        }
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands_at(position: egui::Pos2, release: bool) -> Vec<ViewportCommand> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, false);
        egui_extras::install_image_loaders(&ctx);
        let mut app = KitlaneApp::preview(0, false, "normal");
        let mut commands = Vec::new();
        for frame in 0..=3 {
            let mut input = egui::RawInput {
                time: Some(frame as f64 * 0.02),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1000.0, 700.0),
                )),
                ..Default::default()
            };
            if frame > 0 {
                input.events.push(egui::Event::PointerMoved(position));
            }
            if frame == 2 || (frame == 3 && release) {
                input.events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 2,
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(input, |ui| app.content(ui));
            output.textures_delta.clear();
            commands.extend(
                output
                    .viewport_output
                    .remove(&egui::ViewportId::ROOT)
                    .unwrap()
                    .commands,
            );
        }
        commands
    }

    #[test]
    fn window_controls_send_native_commands() {
        assert!(
            commands_at(egui::pos2(965.0, 30.0), true)
                .iter()
                .any(|v| matches!(v, ViewportCommand::Close))
        );
        assert!(
            commands_at(egui::pos2(912.0, 30.0), true)
                .iter()
                .any(|v| matches!(v, ViewportCommand::Maximized(true)))
        );
        assert!(
            commands_at(egui::pos2(856.0, 30.0), true)
                .iter()
                .any(|v| matches!(v, ViewportCommand::Minimized(true)))
        );
        assert!(
            commands_at(egui::pos2(450.0, 24.0), false)
                .iter()
                .any(|v| matches!(v, ViewportCommand::StartDrag))
        );
        assert!(
            commands_at(egui::pos2(2.0, 300.0), false)
                .iter()
                .any(|v| matches!(v, ViewportCommand::BeginResize(egui::ResizeDirection::West)))
        );
    }
}
