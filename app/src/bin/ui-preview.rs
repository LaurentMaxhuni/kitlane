#[cfg(debug_assertions)]
#[path = "../shell.rs"]
#[allow(dead_code)] // Fixture rendering deliberately never starts the worker.
mod shell;
#[cfg(debug_assertions)]
#[path = "../theme.rs"]
mod theme;
#[cfg(debug_assertions)]
#[path = "../worker.rs"]
#[allow(dead_code)] // Share command types without invoking storage, vault, or networking.
mod worker;

#[cfg(debug_assertions)]
#[path = "../ai_client.rs"]
#[allow(dead_code)]
mod ai_client;
#[cfg(debug_assertions)]
#[path = "../maintenance_agent.rs"]
#[allow(dead_code)]
mod maintenance_agent;
#[cfg(debug_assertions)]
#[path = "../self_update.rs"]
#[allow(dead_code)]
mod self_update;

#[cfg(debug_assertions)]
mod preview {
    use eframe::egui;
    use std::{
        ffi::OsString,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    const USAGE: &str = "ui-preview --page library|devices|settings|welcome|appearance|setup-device|setup-review|pair-service|pair-consent|pair-review|pair-done|sign-in --theme light|dark --state normal|empty|loading|error|long|reduced|details|automation|alternatives|ai-settings|filtered --width 480..4096 --height 360..2160 --output ABSOLUTE.png";
    const TIMEOUT: Duration = Duration::from_secs(15);

    #[derive(Debug, PartialEq)]
    struct Options {
        page: usize,
        dark: bool,
        state: String,
        width: u32,
        height: u32,
        output: PathBuf,
    }

    impl Options {
        fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
            let mut values = std::collections::HashMap::new();
            let mut args = args.into_iter();
            while let Some(key) = args.next() {
                let key = key
                    .into_string()
                    .map_err(|_| "Argument names must be UTF-8")?;
                if !matches!(
                    key.as_str(),
                    "--page" | "--theme" | "--state" | "--width" | "--height" | "--output"
                ) {
                    return Err(format!("Unknown argument: {key}"));
                }
                let value = args
                    .next()
                    .ok_or_else(|| format!("Missing value for {key}"))?;
                if values.insert(key.clone(), value).is_some() {
                    return Err(format!("Duplicate argument: {key}"));
                }
            }
            let mut take = |key: &str| {
                values
                    .remove(key)
                    .ok_or_else(|| format!("Required argument: {key}"))
            };
            let page = match take("--page")?.to_str() {
                Some("library") => 0,
                Some("devices") => 1,
                Some("settings") => 2,
                Some("welcome") => 3,
                Some("appearance") => 4,
                Some("setup-device") => 5,
                Some("setup-review") => 6,
                Some("pair-service") => 7,
                Some("pair-consent") => 8,
                Some("pair-review") => 9,
                Some("pair-done") => 10,
                Some("sign-in") => 11,
                _ => return Err("Unknown preview page. See --help for accepted pages.".into()),
            };
            let dark = match take("--theme")?.to_str() {
                Some("light") => false,
                Some("dark") => true,
                _ => return Err("Theme must be light or dark".into()),
            };
            let state = take("--state")?
                .into_string()
                .map_err(|_| "State must be UTF-8")?;
            if !matches!(
                state.as_str(),
                "normal"
                    | "empty"
                    | "loading"
                    | "error"
                    | "long"
                    | "reduced"
                    | "details"
                    | "automation"
                    | "alternatives"
                    | "ai-settings"
                    | "filtered"
            ) {
                return Err(
                    "State must be normal, empty, loading, error, long, reduced, details, automation, alternatives, ai-settings, or filtered".into(),
                );
            }
            let dimension = |value: OsString, name: &str, min: u32, max: u32| {
                value
                    .to_str()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|value| (min..=max).contains(value))
                    .ok_or_else(|| format!("{name} must be an integer in {min}..={max}"))
            };
            let width = dimension(take("--width")?, "Width", 480, 4096)?;
            let height = dimension(take("--height")?, "Height", 360, 2160)?;
            let output = PathBuf::from(take("--output")?);
            if !output.is_absolute()
                || output
                    .extension()
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("png"))
            {
                return Err("Output must be an absolute path ending in .png".into());
            }
            Ok(Self {
                page,
                dark,
                state,
                width,
                height,
                output,
            })
        }
    }

    struct Capture {
        inner: crate::shell::KitlaneApp,
        output: PathBuf,
        started: Instant,
        frames: u32,
        requested: bool,
        result: Arc<Mutex<Option<Result<(), String>>>>,
    }

    impl Capture {
        fn finish(&self, ctx: &egui::Context, result: Result<(), String>) {
            *self.result.lock().expect("capture result mutex") = Some(result);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    impl eframe::App for Capture {
        fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            if self.started.elapsed() >= TIMEOUT
                && self.result.lock().expect("capture result mutex").is_none()
            {
                self.finish(ctx, Err("Screenshot timed out after 15 seconds".into()));
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            let ctx = ui.ctx().clone();
            let captured = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot {
                        viewport_id, image, ..
                    } = event
                    {
                        (*viewport_id == egui::ViewportId::ROOT).then(|| image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = captured {
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                let result = image::save_buffer_with_format(
                    &self.output,
                    &pixels,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .map_err(|error| format!("Could not save {}: {error}", self.output.display()));
                self.finish(&ctx, result);
            }
            eframe::App::ui(&mut self.inner, ui, frame);
            self.frames += 1;
            if !self.requested
                && self.frames >= 12
                && self.started.elapsed() >= Duration::from_millis(250)
            {
                self.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    pub fn run() -> Result<(), String> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() == 1 && args[0] == "--help" {
            println!("{USAGE}");
            return Ok(());
        }
        let options = Options::parse(args).map_err(|error| format!("{error}\n{USAGE}"))?;
        let result = Arc::new(Mutex::new(None));
        let capture_result = result.clone();
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([options.width as f32, options.height as f32])
                .with_resizable(false),
            renderer: eframe::Renderer::Glow,
            persist_window: false,
            ..Default::default()
        };
        eframe::run_native(
            "Kitlane UI preview",
            native_options,
            Box::new(move |cc| {
                crate::theme::install_fonts(&cc.egui_ctx);
                crate::theme::apply(&cc.egui_ctx, options.dark);
                egui_extras::install_image_loaders(&cc.egui_ctx);
                cc.egui_ctx.set_pixels_per_point(1.0);
                Ok(Box::new(Capture {
                    inner: crate::shell::KitlaneApp::preview(
                        options.page,
                        options.dark,
                        &options.state,
                    ),
                    output: options.output,
                    started: Instant::now(),
                    frames: 0,
                    requested: false,
                    result: capture_result,
                }))
            }),
        )
        .map_err(|error| format!("Native preview failed: {error}"))?;
        result
            .lock()
            .expect("capture result mutex")
            .take()
            .unwrap_or_else(|| Err("Preview closed before screenshot completed".into()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn args() -> Vec<OsString> {
            let output = std::env::temp_dir().join("kitlane-fixture.png");
            [
                "--page", "library", "--theme", "light", "--state", "normal", "--width", "1000",
                "--height", "680", "--output",
            ]
            .map(OsString::from)
            .into_iter()
            .chain([output.into_os_string()])
            .collect()
        }

        #[test]
        fn accepts_every_page_theme_and_state() {
            for (page, index) in [("library", 0), ("devices", 1), ("settings", 2)] {
                for theme in ["light", "dark"] {
                    for state in [
                        "normal",
                        "empty",
                        "loading",
                        "error",
                        "long",
                        "ai-settings",
                        "filtered",
                    ] {
                        let mut args = args();
                        args[1] = page.into();
                        args[3] = theme.into();
                        args[5] = state.into();
                        let parsed = Options::parse(args).unwrap();
                        assert_eq!(parsed.page, index);
                        assert_eq!(parsed.dark, theme == "dark");
                        assert_eq!(parsed.state, state);
                    }
                }
            }
        }

        #[test]
        fn rejects_invalid_dimensions_output_and_options() {
            for (index, value) in [
                (1, "missing"),
                (3, "auto"),
                (5, "secret"),
                (7, "0"),
                (7, "4097"),
                (9, "2161"),
                (9, "360.5"),
                (11, "relative.png"),
            ] {
                let mut args = args();
                args[index] = value.into();
                assert!(Options::parse(args).is_err());
            }
            let mut wrong_extension = args();
            wrong_extension[11] = std::env::temp_dir().join("fixture.jpg").into_os_string();
            assert!(Options::parse(wrong_extension).is_err());
            let mut duplicate = args();
            duplicate.extend([OsString::from("--page"), OsString::from("devices")]);
            assert!(Options::parse(duplicate).is_err());
            assert!(Options::parse([OsString::from("--unknown")]).is_err());
            assert!(Options::parse([OsString::from("--page")]).is_err());
            assert!(Options::parse(Vec::new()).is_err());
        }
    }
}

#[cfg(debug_assertions)]
fn main() {
    if let Err(error) = preview::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("ui-preview requires a debug build");
    std::process::exit(1);
}
