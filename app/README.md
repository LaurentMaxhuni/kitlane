# Kitlane app

Native Rust desktop application using [egui/eframe](https://docs.rs/eframe/). First-launch onboarding configures appearance, device name, and scan timing. The Library scans platform inventory in the background, saves it locally, and shows vendor-evidenced OS availability for a small bundled catalog. Search matches app names and publishers. Devices supports explicit local-service enrollment and sync, followed by review-only compatibility proposals. Settings saves appearance and scan preferences.

Inventory coverage: [Windows uninstall registry](docs/windows-inventory.md), [macOS application bundles](docs/macos-inventory.md), and [Linux Flatpak applications](docs/linux-inventory.md). Store/MSIX, portable software, other users, and Linux native packages are outside these adapters. macOS/Linux code has fixture and cross-compilation checks; native runtime validation remains pending. Catalog associations use metadata, not verified binary identity. Unknown apps remain unknown. Installation, updates, and AI suggestions are not enabled.

Registry size checks are not atomic: another local process can enlarge a value while it is read. See [scanner coverage and limits](docs/windows-inventory.md).

Requires Rust 1.95 or newer and your platform's native linker (MSVC Build Tools on Windows). Linux builds also need D-Bus development libraries (`libdbus-1-dev` on Debian/Ubuntu); credential storage requires a running Secret Service and user session. Run from this folder:

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

Local state is stored at `%LOCALAPPDATA%/Kitlane/state.redb` on Windows. Existing invalid or unsupported-schema databases produce an error instead of being overwritten. Inventory stays local until an enrolled user explicitly consents and clicks Sync now. Device credentials use the OS credential store; there is no plaintext fallback.

Run a read-only inventory diagnostic that prints counts without app names or paths:

```powershell
cargo run --bin inventory-check
```

## Installation and first launch

Windows releases use a branded Inno Setup wizard with system light/dark appearance, install-location selection, an optional desktop shortcut, and a launch option. Installation is per-user without administrator rights. Uninstall removes application files and shortcuts while retaining `%LOCALAPPDATA%/Kitlane` inventory and preferences.

Build the x64 installer on Windows with Inno Setup 6.7+:

```powershell
./installer/build.ps1
# A portable compiler can be supplied explicitly:
./installer/build.ps1 -Compiler ./target/inno/ISCC.exe
```

The build script generates branded artwork from shared design tokens and emits `../dist/Kitlane-<version>-Setup-x64.exe`. Windows CI builds and uploads this artifact after Rust checks. Local builds are unsigned; release signing needs a separately configured certificate. macOS and Linux installer packaging remains future work.

On first launch, all platforms show a four-step setup: welcome/privacy, light or dark appearance, device name and scan timing, then a review with optional navigation to device pairing. Each Continue/Back saves choices and progress atomically. Setup resumes after interruption and closes only after the final database write succeeds. Scanning starts only after setup completes and only if selected; sharing still needs consent and Sync now. Existing appearance-only settings remain readable and show setup once. Settings provides scan-on-launch control and Run setup again.

Generate diagnostic screenshots without touching real user state:

```powershell
cargo run --bin ui-preview -- --page setup-device --theme dark --state normal --width 1000 --height 700 --output C:/path/to/setup.png
```

Preview pages also include `welcome`, `appearance`, and `setup-review`.

The separate Rust [sync service](sync-server/README.md) provides a local single-owner enrollment/snapshot prototype connected through the [Rust client](sync-client/README.md). In Devices, copy the device identity for the owner, obtain a pairing grant, select inventory-sharing consent, and enroll. Check saved enrollment after restarting, then Sync now to upload this snapshot and retrieve peers. Forget credential removes only the local credential; server revocation requires the owner. Lost credentials can be recovered with an owner-issued recovery grant under the same device ID. Service endpoints are limited to `http://127.0.0.1:<port>`; connecting physical computers remotely and hosted accounts remain future work.

`build.rs` validates and converts shared semantic color tokens into Rust constants at compile time. Inter is embedded once in the executable; neither fonts nor tokens need runtime file access. Keep the font's bundled OFL license with distributed releases. Cargo.lock pins dependency versions; target/ is generated and ignored.

Shared assets, tokens, and design guidance live in [`../design-system/`](../design-system/README.md).

See [implementation research](docs/implementation-research.md) for platform adapters, identity matching, sync, AI alternatives, safety boundaries, and delivery sequence.
