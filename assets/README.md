# Asset library

## Logo files

`logos/kitlane-logo.svg` is the primary horizontal lockup. The matching PNG is transparent and 1600 pixels wide. Reversed files replace navy lettering with white while preserving cyan and mint. Monochrome files are available in navy and white. Symbol and wordmark exports can be used separately.

SVG paths were traced from the approved raster source and smoothed to remove pixel stair steps. They retain the source's lettering rather than replacing it with a font approximation. All variants share the same geometry. Use SVG for scalable interfaces and PNG where vector assets are unsupported.

## Desktop icons

- `icons/app-icon.svg`: scalable, transparent exterior with the original navy rounded-square icon.
- `icons/app-icon-{size}.png`: 16, 24, 32, 48, 64, 128, 256, 512, and 1024 pixel square canvases.
- `icons/app-icon.ico`: Windows container with 16–256 pixel PNG entries.
- `icons/app-icon.icns`: macOS container with 16–1024 pixel entries.
- `icons/tray-icon.svg` and `tray-icon-white.svg`: monochrome transparent marks.
- Matching tray PNGs: 16, 24, and 32 pixels for light or dark system chrome.

Keep the platform's icon mask and safe area in mind. This set is suitable for initial desktop integration; inspect the 16 px tray version on the actual target OS and adjust to that platform's optical requirements when the desktop shell is implemented.

## Fonts

Sora's normal variable font covers weights 100–800. Inter's normal and italic variable fonts cover weights 100–900 with optical sizing. The CSS declarations use local assets; no Google Fonts request is made at runtime. OFL license texts accompany both families. Source links are in the project README.

## Interface icons

`ui-icons/` contains a focused subset of Lucide icons. Keep an 18 px default icon box and consistent outline treatment. Use a meaningful button label or accessible name; decorative icons are hidden from assistive technology. The Lucide license is included.

## Brand artwork

`brand/brand-pattern.svg` repeats the source-traced K geometry at low emphasis for occasional marketing use. `brand/brand-cover.svg` and the matching 1200 × 630 PNG provide a simple navy identity cover. Keep patterns away from dense interface content.

## Asset manifest

`manifest.json` inventories exported and source assets with file sizes and SHA-256 checksums, font provenance, and the logo source. Rebuild assets after changing an asset file so the manifest stays current.

## Source files

`source/kitlane-original.png` is the unmodified supplied image. `source/brand-board.png` is the earlier generated identity overview. The written docs and tokens are authoritative for implementation; the overview is visual direction.

