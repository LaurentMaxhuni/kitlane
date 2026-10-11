# Kitlane app

Native Rust desktop application using [egui/eframe](https://docs.rs/eframe/). The Library scans Windows uninstall-registry entries in the background, saves inventory locally, and shows vendor-evidenced OS availability for a small bundled catalog. Search matches app names and publishers. Devices shows the persistent local device identity; Settings saves appearance preferences.

Inventory coverage is explicit: this pass does not scan MSIX/Store or portable applications, and macOS/Linux scanners are not implemented yet. Catalog associations use metadata, not verified binary identity. Unknown apps remain unknown. Installation, updates, and AI suggestions are not enabled.

Registry size checks are not atomic: another local process can enlarge a value while it is read. See [scanner coverage and limits](docs/windows-inventory.md).

Requires Rust 1.95 or newer and your platform's native linker (MSVC Build Tools on Windows). Run from this folder:

```powershell
cargo run
cargo build --release --bin kitlane
```

Verify:

```powershell
cargo fmt --check
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
```

The GUI uses eframe with OpenGL rendering and native accessibility. `core/` supplies shared models, redb persistence, inventory, and catalog matching. One dedicated background worker serializes scanning and database access; bounded commands and result notifications keep the GUI responsive without continuous polling/repainting. Release builds use thin LTO and one codegen unit. Performance still needs measurement against real workloads.

Local state is stored at `%LOCALAPPDATA%/Kitlane/state.redb` on Windows. Existing invalid or unsupported-schema databases produce an error instead of being overwritten. Inventory is not uploaded by the desktop app.

Run a read-only inventory diagnostic that prints counts without app names or paths:

```powershell
cargo run --bin inventory-check
```

The separate Rust [sync service](sync-server/README.md) provides a local single-owner enrollment/snapshot prototype. It is not a deployed account service and is not connected to the desktop UI yet. Follow its authentication and loopback-only setup instructions before testing.

`build.rs` validates and converts shared semantic color tokens into Rust constants at compile time. Inter is embedded once in the executable; neither fonts nor tokens need runtime file access. Keep the font's bundled OFL license with distributed releases. Cargo.lock pins dependency versions; target/ is generated and ignored.

Shared assets, tokens, and design guidance live in [`../design-system/`](../design-system/README.md).

See [implementation research](docs/implementation-research.md) for platform adapters, identity matching, sync, AI alternatives, safety boundaries, and delivery sequence.
