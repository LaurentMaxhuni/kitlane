# Kitlane app

Native Rust desktop application using [egui/eframe](https://docs.rs/eframe/). Verified Google login through Neon is required before onboarding or the workspace. First-launch onboarding configures appearance, device name, and scan timing. The Library scans platform inventory in the background, saves it locally, and shows vendor-evidenced OS availability for a small bundled catalog. Search matches app names and publishers. Devices uses account-scoped pairing codes and explicit inventory sync, followed by review-only compatibility proposals. Settings saves appearance/scan preferences and provides Sign out.

Inventory coverage: [Windows uninstall registry](docs/windows-inventory.md), [macOS application bundles](docs/macos-inventory.md), and [Linux Flatpak applications](docs/linux-inventory.md). Store/MSIX, portable software, other users, and Linux native packages are outside these adapters. macOS/Linux code has fixture and cross-compilation checks; native runtime validation remains pending. Catalog associations use metadata, not verified binary identity. Unknown apps remain unknown. Installation, updates, and AI suggestions are not enabled.

Registry size checks are not atomic: another local process can enlarge a value while it is read. See [scanner coverage and limits](docs/windows-inventory.md).

Requires Rust 1.95 or newer and your platform's native linker (MSVC Build Tools on Windows). Linux builds also need D-Bus development libraries (`libdbus-1-dev` on Debian/Ubuntu); credential storage requires a running Secret Service and user session. Run from this folder:

```powershell
$env:KITLANE_ACCOUNT_API = 'https://YOUR-DEPLOYED-ACCOUNT-API'
cargo run
cargo build --release --bin kitlane
```

The API address is compiled into the desktop and must be an exact HTTPS origin. Missing configuration keeps the login gate closed with a setup message. See [account service setup](../services/account-api/README.md) and [native client](account-client/README.md). The development service and real Google sign-in have been verified; see [deployment evidence and remaining checks](docs/development-deployment.md). Public previews use that development API and shared development OAuth. Production requires separate Google OAuth configuration.

Verify:

```powershell
cargo fmt --check
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
```

The GUI uses eframe with OpenGL rendering and native accessibility. `core/` supplies shared models, redb persistence, inventory, and catalog matching. One dedicated background worker serializes scanning and database access; bounded commands and result notifications keep the GUI responsive without continuous polling/repainting. Release builds use thin LTO and one codegen unit. Performance still needs measurement against real workloads.

Local state is stored at `%LOCALAPPDATA%/Kitlane/state.redb` on Windows. Existing invalid or unsupported-schema databases produce an error instead of being overwritten. Inventory stays local until a signed-in user explicitly consents and clicks Sync now. Account credentials use the OS credential store; there is no plaintext fallback. Sign-out clears cloud devices, transient codes, and sharing consent while preserving local inventory/preferences.

Run a read-only inventory diagnostic that prints counts without app names or paths:

```powershell
cargo run --bin inventory-check
```

## Installation and first launch

Windows releases use a branded Inno Setup wizard with system light/dark appearance, install-location selection, an optional desktop shortcut, and a launch option. Installation is per-user without administrator rights. Uninstall removes application files and shortcuts while retaining `%LOCALAPPDATA%/Kitlane` inventory and preferences.

Build the x64 installer on Windows with Inno Setup 6.7+:

```powershell
./installer/build.ps1 -AccountApi 'https://YOUR-VERIFIED-ACCOUNT-API'
# A portable compiler can be supplied explicitly:
./installer/build.ps1 -Compiler ./target/inno/ISCC.exe -AccountApi 'https://YOUR-VERIFIED-ACCOUNT-API'
```

The build script requires a verified HTTPS API origin (argument or `KITLANE_ACCOUNT_API`) and always rebuilds the executable with that address before packaging. It generates branded artwork from shared design tokens and emits `../dist/Kitlane-<version>-Setup-x64.exe`. The ordinary Rust checks workflow packages Windows only when repository variable `KITLANE_ACCOUNT_API` is configured. Windows installers are unsigned; release signing needs a separately configured certificate.

### Cross-platform previews

`.github/workflows/release.yml` tests and packages Windows x64, macOS Apple Silicon/Intel, and Linux x64/ARM64 on native runners. Matching `v<version>` tags publish a GitHub prerelease with all five packages and `SHA256SUMS.txt` only after every target passes. Manual branch runs save build artifacts without publishing. The preview workflow explicitly configures the verified development API.

On macOS or Linux, set `KITLANE_ACCOUNT_API` and run `node app/installer/package.mjs` from the repository root (Node 22+). It rebuilds the configured executable. macOS packaging uses system `codesign`/`hdiutil` to produce an ad-hoc-signed `.app` in a verified `.dmg`, with icon, version metadata, and licenses. No Apple Developer certificate or notarization is configured; macOS may block the download. Linux packaging creates a `.tar.gz` with executable, icon, runtime requirements, and licenses. Linux x64 builds target Ubuntu 22.04/glibc 2.35+; ARM64 targets Ubuntu 24.04/glibc 2.39+. Both require a graphical session, OpenGL, D-Bus, and an unlocked Secret Service keyring. Interactive macOS/Linux launch, sign-in, and scanning checks remain pending.

After Google login, first launch shows a four-step setup: welcome, light or dark appearance, device name and scan timing, then review with optional device pairing. Each Continue/Back saves choices and progress atomically. Setup resumes after interruption and closes only after the final database write succeeds. Scanning starts only after setup completes and only if selected; sharing needs consent and Sync now. Existing appearance-only settings remain readable and show setup once. Settings provides scan-on-launch control and Run setup again.

Generate diagnostic screenshots without touching real user state:

```powershell
cargo run --bin ui-preview -- --page setup-device --theme dark --state normal --width 1000 --height 700 --output C:/path/to/setup.png
```

Preview pages also include `welcome`, `appearance`, and `setup-review`.

## Desktop interactions

The native window uses a branded title bar with appearance switching, drag, double-click maximize/restore, labeled window controls, and resize targets on all edges. Navigation and control feedback use short transitions; page and wizard steps fade in, and detail sections expand. Settings saves Reduced motion, which makes transitions immediate, disables programmatic scroll animation, and uses static loading indicators. Idle screens do not continuously repaint. Scrollbars remain thin and subdued until hovered.

Library includes All apps / Catalog matches / Unmatched filters, Clear search, and clickable names that reveal expandable details and Copy app information. On Windows, scanning reads installed programs' local icon resources using their registry DisplayIcon, installation directory, or uninstall executable metadata; it never runs those executables. Bounded icon decoding uses `image` and `pelite`. Generated PNGs are cached in `%LOCALAPPDATA%/Kitlane/icons` and stay local; paths and icon bytes never enter inventory snapshots. A local diagnostic (`cargo run --bin icon-check`) reports aggregate coverage without writing inventory or credentials. Programs without usable resources fall back to embedded official logos for Visual Studio Code, VLC, Notepad++, and Blender, then initial tiles. See [asset sources and attribution](assets/programs/NOTICE.md), also included in Windows installations. Other platforms currently use the embedded logos and initial tiles.

Devices lists explicitly paired peers, including computers that have not shared inventory. An empty list centers Connect a device. The padded wizard offers Create code / Enter code, shows the signed-in account, validates eight-character codes, and provides expiry/copy/regeneration feedback. Codes last five minutes and can be consumed once by another device on the same account. Pairing never uploads inventory. After pairing, users can choose sharing consent and Sync now; Sync libraries reopens those controls. Shared snapshots include inventory age and expandable compatibility proposals. Devices are connections rather than live online-presence indicators; native window behavior outside Windows remains unverified.

Diagnostic pages `sign-in`, `pair-service`, `pair-consent`, `pair-review`, and `pair-done` render login/wizard stages without starting the worker. States `details` and `reduced` exercise app details and reduced motion. Normal Devices fixtures include a synthetic peer; `empty` renders the CTA. Actions needing a worker remain disabled in screenshots.

The separate Rust [sync service](sync-server/README.md) and [loopback client](sync-client/README.md) remain development prototypes. The release GUI uses the Neon account API exclusively. Legacy owner grants are not migrated or accepted as Google sessions.

`build.rs` validates and converts shared semantic color tokens into Rust constants at compile time. Inter is embedded once in the executable; neither fonts nor tokens need runtime file access. Keep the font's bundled OFL license with distributed releases. Cargo.lock pins dependency versions; target/ is generated and ignored.

Shared assets, tokens, and design guidance live in [`../design-system/`](../design-system/README.md).

See [implementation research](docs/implementation-research.md) for platform adapters, identity matching, sync, AI alternatives, safety boundaries, and delivery sequence.
