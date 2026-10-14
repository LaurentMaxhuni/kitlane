# Kitlane website

Self-contained, dependency-free marketing website, separate from the Rust app and shared design-system tooling. Landing page at `/`; download page at `/download/`.

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

Upload the contents of `website/dist/` to a static host. Keep `download/index.html` and all assets together; directory index hosting supports `/download/`. Paths also work when deployed under a subdirectory. No external fonts, third-party scripts, trackers, API requests, or install step.

## Release status

The download page deliberately links to GitHub releases instead of distributing the existing local installer. The app documentation requires account sign-in and packaged-build verification before public distribution. macOS/Linux installer packaging and native runtime verification remain pending.

When a verified Windows release is published, update `download/index.html`: replace the release-status link with the published HTTPS installer URL, change its label to “Download for Windows”, update version/platform metadata and status, and replace the preparation message with verified release details. Publish only assets from the verified release; never copy development executables into the website. Keep macOS/Linux unavailable until their releases exist.

## Assets

Kitlane artwork and Sora/Inter fonts are copied from `design-system/assets/`. Their OFL licenses are retained under `assets/fonts/`. Lucide icons retain `assets/icons/LICENSE`. Unmodified third-party app logos retain `assets/programs/NOTICE.md`; their inclusion identifies illustrative library entries and does not imply endorsement.

`assets/library.png` and `assets/devices.png` are genuine native app previews rendered using the existing `ui-preview` diagnostic with synthetic inventory. They contain no user inventory or credentials. Refresh them after app UI changes:

```powershell
app/target/debug/ui-preview.exe --page library --theme light --state normal --width 1440 --height 900 --output C:/github-projects/kitlane/website/assets/library.png
app/target/debug/ui-preview.exe --page devices --theme light --state normal --width 1200 --height 800 --output C:/github-projects/kitlane/website/assets/devices.png
```

The landing-page preview switch, mobile navigation, and native FAQ disclosures support keyboard use. Reduced motion disables smooth scrolling and transitions. Main navigation and FAQ content remain usable with JavaScript disabled.
