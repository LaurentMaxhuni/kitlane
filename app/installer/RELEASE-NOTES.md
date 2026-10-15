Native Kitlane 0.1.3 preview for Windows x64, macOS Apple Silicon/Intel, and Linux x64/ARM64.

Google sign-in and internet access required. This preview connects to the verified **development** account service, using shared development Google OAuth. Automatic app installation and updates are not included.

- **Windows:** run `Kitlane-0.1.3-Setup-x64.exe`. Per-user installation; Windows 10 1809 or newer, x64.
- **macOS:** open the matching `.dmg`, then drag Kitlane into Applications. macOS 12 or newer. `arm64` is for Apple Silicon; `x64` is for Intel. Packages have an ad-hoc signature, with no Apple Developer signing or notarization. macOS may block them; consult [Apple's guidance](https://support.apple.com/en-us/102445) and your device policy.
- **Linux:** extract the matching `.tar.gz` and run `./kitlane`. x64 builds require glibc 2.35+; ARM64 builds require glibc 2.39+. Requires a graphical session, OpenGL, D-Bus, and an unlocked Secret Service keyring. See bundled README for runtime packages. Inventory covers Flatpak apps.

Native tests, lint, and package structure checks run on each target in GitHub Actions. Interactive macOS/Linux launch, sign-in, and inventory checks remain pending, as do physical two-computer and real foreign-account browser checks. Windows install/uninstall and real Google sign-in were verified during development. Windows installers are unsigned.

Inventory stays local until you explicitly consent and sync. Uninstalling keeps local inventory and preferences. Downloads include third-party license notices. `SHA256SUMS.txt` records SHA-256 checksums for every download.
