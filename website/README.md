# Kitlane website

Dependency-free marketing website, separate from the Rust app and shared design-system tooling. Landing page at `/`; download page at `/download/`. A small Vercel Node function resolves published downloads.

Production: [kitlane-lake.vercel.app](https://kitlane-lake.vercel.app/) · [Downloads](https://kitlane-lake.vercel.app/download/). Vercel deploys `main` from the connected GitHub repository.

Requires Node.js 22 for the development server, build scripts and download function.

```powershell
cd website
npm run dev
```

Open **http://localhost:4174**. `PORT` overrides the preview port.

```powershell
npm run check
npm run build
```

Vercel uses project root `website`, `npm run build`, and static output `dist`, plus `api/download.js`; `vercel.json` records the build settings. The build checks local references and runs the download tests before producing output. Deploy the project through Vercel's Git integration so it includes the API function; copying only `dist` to a static host does not provide automatic downloads. No external fonts, third-party scripts, trackers, dependencies or install step. Native downloads stay on GitHub Releases, outside the website bundle.

## Release status

The page uses permanent `/api/download?platform=...` URLs for Windows x64, macOS Apple Silicon/Intel, Linux x64/ARM64, checksums and release notes. Without `platform`, the endpoint returns the selected version and download URLs as JSON, which updates the page's version labels. Download links work with JavaScript disabled.

The resolver checks the 100 most recent GitHub releases and chooses the highest numeric `v<major>.<minor>.<patch>` version, including previews. It skips drafts and incomplete uploads: all five expected packages and `SHA256SUMS.txt` must be uploaded, nonempty, unique and hosted at their exact official release URLs. Shared lookups expire after 60 seconds; redirects and JSON use `no-store`, with no stale fallback after lookup failure. Failures return 503 with a retry message. This checks release completeness, not installer publisher certificates or update signatures; Kitlane's built-in updater verifies its signed metadata separately.

Public GitHub lookup needs no credential. For higher traffic, configure optional Vercel server-only `GITHUB_RELEASES_TOKEN` with read-only Contents access to this repository to avoid GitHub's unauthenticated API limit. Never expose it in page scripts. No credential is required by the local build or fixture tests.

For each release, update `app/Cargo.toml` and release notes, then push a matching `v<version>` tag. `.github/workflows/release.yml` tests and packages all five targets, signs Windows update metadata using `KITLANE_UPDATE_SIGNING_KEY`, and publishes one prerelease after every target succeeds. A missing CI signing secret blocks publication. `workflow_dispatch` on a branch builds artifacts without publishing. Once publication is complete, the website discovers that version within a minute without edits or redeployment. Platform requirements and feature descriptions still need review when support changes. Never copy development executables into the website.

The packages use the verified development account API and shared development Google OAuth. Interactive macOS/Linux and physical two-computer checks remain pending. Windows downloads have no publisher certificate; Mac downloads have ad-hoc signatures and no Apple notarization. The page states these limits.

## Assets

Kitlane artwork and Sora/Inter fonts are copied from `design-system/assets/`. Their OFL licenses are retained under `assets/fonts/`. Lucide icons retain `assets/icons/LICENSE`. Unmodified third-party app logos retain `assets/programs/NOTICE.md`; their inclusion identifies illustrative library entries and does not imply endorsement.

`assets/library.png` and `assets/devices.png` are genuine native app previews rendered using the existing `ui-preview` diagnostic with synthetic inventory. They contain no user inventory or credentials. Refresh them after app UI changes:

```powershell
app/target/debug/ui-preview.exe --page library --theme light --state normal --width 1440 --height 900 --output C:/github-projects/kitlane/website/assets/library.png
app/target/debug/ui-preview.exe --page devices --theme light --state normal --width 1200 --height 800 --output C:/github-projects/kitlane/website/assets/devices.png
```

The landing-page preview switch, mobile navigation, and native FAQ disclosures support keyboard use. Reduced motion disables smooth scrolling and transitions. Main navigation and FAQ content remain usable with JavaScript disabled.
