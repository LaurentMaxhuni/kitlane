# Native UI review

`ui-preview` renders the actual Kitlane egui shell in a native eframe window using sanitized fixtures. It is available only in debug builds. It does not construct the production app, start inventory workers, read a vault, or connect to sync services.

Run from `app` with an absolute PNG output path. Create the output directory first:

```powershell
cargo run --bin ui-preview -- --page library --theme light --state normal --width 1360 --height 900 --output C:\temp\kitlane-library-light.png
cargo run --bin ui-preview -- --page devices --theme dark --state normal --width 1000 --height 680 --output C:\temp\kitlane-devices-dark.png
cargo run --bin ui-preview -- --page settings --theme light --state normal --width 480 --height 640 --output C:\temp\kitlane-settings-narrow.png
```

All six options are required. Pages: `library`, `devices`, `settings`. Themes: `light`, `dark`. Fixture states: `normal`, `empty`, `loading`, `error`, `long`. Width bounds: 480–4096; height bounds: 360–2160. Existing output files are replaced. `--help` prints usage without opening a window.

The wrapper installs the production fonts/theme and image loaders, requests a viewport screenshot after at least 12 frames and 250 ms, writes the returned RGBA pixels as PNG, and closes. UI scaling is fixed at one pixel per point. Native window sizing can still be constrained by the display, so compare actual image dimensions. A missing screenshot event times out after 15 seconds; rendering or saving errors exit unsuccessfully. The harness requires a working desktop session and graphics driver.

## Review matrix

Capture every page in both themes at 1360×900 and 480×640. Inspect Library and Devices with each fixture state, including long labels at narrow width. Check:

- Navigation, primary action, app names, versions, publishers, and statuses remain readable.
- Light/dark surfaces, text, separators, selected navigation, and focus indicators keep useful contrast.
- Native scrolling reveals all content without clipped labels or overlapping controls.
- Empty/loading/error states explain current status and next action consistently.
- Device differences, scan timestamps, and warnings remain visible.
- Settings inputs and action labels remain usable at narrow width.

Keep earlier screenshots as a baseline and compare at matching page/theme/state/size. The harness records pixels; review layout changes visually rather than treating renderer-dependent antialiasing differences as failures.

Run parser validation with `cargo test --bin ui-preview preview::tests`. Capture generation does not prove production actions work: test keyboard traversal, text input, cancellation, vault locking, and normal scan/sync behavior separately with disposable data. Never use screenshots containing real device identities, installed software, credentials, or vault material as review fixtures.

API references: cached egui 0.36.2 `viewport.rs` defines `ViewportCommand::Screenshot(UserData)`; `data/input/event.rs` returns `Event::Screenshot { viewport_id, user_data, image }`. eframe 0.36.2 `epi.rs` defines `App::logic` and `App::ui`.
