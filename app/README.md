# Kitlane app

Native Rust desktop application using [egui/eframe](https://docs.rs/eframe/). Window, UI, state, and theme integration are Rust. The scaffold includes Library, Devices, and Settings screens with working navigation and a theme toggle. Discovery, installation, pairing, and sync are not implemented yet.

Requires Rust 1.95 or newer and your platform's native linker (MSVC Build Tools on Windows). Run from this folder:

```powershell
cargo run
cargo build --release
```

Verify:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

One runtime dependency: eframe with OpenGL rendering and native accessibility. Default features are disabled; Linux X11 and Wayland are enabled. No continuous repaint request, polling loop, or background worker is added. Release builds use thin LTO and one codegen unit. Performance still needs measurement against real workloads.

`build.rs` validates and converts shared semantic color tokens into Rust constants at compile time. Inter is embedded once in the executable; neither fonts nor tokens need runtime file access. Keep the font's bundled OFL license with distributed releases. Cargo.lock pins dependency versions; target/ is generated and ignored.

Shared assets, tokens, and design guidance live in [`../design-system/`](../design-system/README.md).
