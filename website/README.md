# Kitlane website

Self-contained, dependency-free marketing website, separate from the Rust app and shared design-system tooling. Landing page at `/`; download page at `/download/`.

Production: [kitlane-lake.vercel.app](https://kitlane-lake.vercel.app/) · [Downloads](https://kitlane-lake.vercel.app/download/). Vercel deploys `main` from the connected GitHub repository.

Requires Node.js 22+ for the development server and build scripts. The deployed pages require only static hosting.

```powershell
cd website
npm run dev
```

Open **http://localhost:4174**. `PORT` overrides the preview port.

```powershell
npm run check
npm run build
```

Upload the contents of `website/dist/` to a static host. Keep `download/index.html` and all assets together; directory index hosting supports `/download/`. Paths also work when deployed under a subdirectory. No external fonts, third-party scripts, trackers, API requests, or install step. Vercel uses project root `website`, `npm run build`, and output `dist`; `vercel.json` records the static build settings. Native downloads are hosted on GitHub Releases, outside the website bundle.

## Release status

The download page links to the versioned `v0.1.3` GitHub preview release: Windows x64 setup, macOS Apple Silicon/Intel disk images, Linux x64/ARM64 archives, and SHA-256 checksums. These builds use the verified development account API and shared development Google OAuth. Windows install/uninstall and real sign-in were verified during development. Interactive macOS/Linux and physical two-computer checks remain pending. Windows downloads are unsigned; Mac downloads have ad-hoc signatures and no Apple notarization. The page states these limits.

For each release, update `app/Cargo.toml`, release notes, and the website links/version/platform requirements. Push a matching `v<version>` tag. `.github/workflows/release.yml` builds and tests on native runners, packages all five targets, and publishes one prerelease only after every target succeeds. `workflow_dispatch` builds artifacts without publishing when run on a branch. Verify every website download URL and checksum before deploying the site. Never copy stale development executables into the website.

## Assets

Kitlane artwork and Sora/Inter fonts are copied from `design-system/assets/`. Their OFL licenses are retained under `assets/fonts/`. Lucide icons retain `assets/icons/LICENSE`. Unmodified third-party app logos retain `assets/programs/NOTICE.md`; their inclusion identifies illustrative library entries and does not imply endorsement.

`assets/library.png` and `assets/devices.png` are genuine native app previews rendered using the existing `ui-preview` diagnostic with synthetic inventory. They contain no user inventory or credentials. Refresh them after app UI changes:

```powershell
app/target/debug/ui-preview.exe --page library --theme light --state normal --width 1440 --height 900 --output C:/github-projects/kitlane/website/assets/library.png
app/target/debug/ui-preview.exe --page devices --theme light --state normal --width 1200 --height 800 --output C:/github-projects/kitlane/website/assets/devices.png
```

The landing-page preview switch, mobile navigation, and native FAQ disclosures support keyboard use. Reduced motion disables smooth scrolling and transitions. Main navigation and FAQ content remain usable with JavaScript disabled.
