# Implementation

## Package boundary

This is the design-system foundation for Kitlane. It can be used by a web frontend, a Tauri webview, or an Electron renderer. Native installation, account sync, and software matching belong to the future app implementation.

The system currently supplies CSS components and DOM helpers, without choosing a React/Vue/Svelte architecture. Wrap the documented semantic markup in your chosen framework. Keep native element semantics and the existing class contracts.

## Tokens and themes

`tokens/tokens.json` is the editable source. Foundation entries use typed values, while `theme.light` and `theme.dark` map semantic roles to foundation references or theme-specific colors. Run `pnpm build:tokens` to generate CSS; do not hand-edit `styles/tokens.css`.

Use `--kl-text`, `--kl-muted`, `--kl-bg`, `--kl-surface`, `--kl-surface-raised`, `--kl-border`, `--kl-action`, `--kl-action-text`, and `--kl-focus` for UI roles. Use raw brand tokens only for artwork and intentional brand treatments. Set the theme on an ancestor shared by dialogs and toast regions, ideally the document root.

The reference stores the chosen theme in local storage. The exported `setTheme()` helper does not persist preferences, allowing the app to use its own settings store. System mode listens to media-query changes and returns a cleanup function.

## Fonts and logos

Self-hosted font URLs are relative to `styles/fonts.css`. Preserve the folder structure or adjust URLs in your build. Keep the bundled OFL licenses when redistributing the fonts. The raster wordmark is not a verified font sample: use the traced logo asset for the brand signature and Sora for live brand text.

SVG exports preserve the original artwork's shape through source tracing. PNGs are rendered from the same SVG geometry. ICO and ICNS containers are generated from the PNG exports. `scripts/export-assets.mjs` is the reproducible asset preparation step; it also copies the licensed Lucide icon subset.

## State and behavior

Form validation, network errors, update eligibility, and match quality must come from actual application state. The reference uses illustrative data. Use the correct status label and present the source and scope of any verification. Never infer platform support from the mere presence of an app in a synced profile.

Toast messages use plain text. For recovery steps, prefer a persistent inline alert with a specific action. Use native `dialog.showModal()` for confirmation. Real long-running updates should expose progress/cancellation state through app logic; the demo's progress is illustrative.

## Density and responsiveness

Use the 4 px spacing scale. Desktop layouts should prioritize readable rows and clear grouping. Compact buttons are suitable for app-list actions; ordinary forms use 42 px controls. At narrow widths, allow sections to stack and keep critical app labels readable. Preserve table semantics rather than converting table cells into unrelated cards.

## Maintenance

Change source tokens first, then regenerate CSS. Regenerate all logo variants when changing approved source geometry. Run `pnpm check` after token or asset changes, then inspect the reference in both themes and at actual icon sizes. Record meaningful changes in `CHANGELOG.md`.
