# Visual foundations

## Type

Sora is the display family and Inter is the UI family. The logo artwork retains its original lettering. Use 800 for the brand specimen, 600–700 for display headings, 400 for body text, 500 for labels, and 600 for primary actions.

| Style | Size | Line height | Use |
| --- | --- | --- | --- |
| Display | 48 px | 1.15 | Marketing or a large overview heading |
| Title | 32 px | 1.3 | Main page title |
| Heading | 24 px | 1.3 | Major section heading |
| Lead | 18 px | 1.5 | Short introductory text |
| Body | 16 px | 1.5 | Explanations and reading |
| UI | 14 px | 1.5 | Controls, lists, and app details |
| Caption | 12 px | 1.5 | Help, metadata, and versions |

Use sentence case. Keep paragraphs around 60–70 characters wide. Allow the display style to scale down on narrow screens; avoid shrinking ordinary controls below their UI baseline.

## Space and layout

Use the 4 px rhythm. Tokens cover 4, 8, 12, 16, 20, 24, 32, 40, 48, and 64 px. Use 8 px within an action, 12 px between related controls, 16 px inside a group, 24–32 px between groups, and 48–64 px between major sections.

Prefer one grouped surface for an app list, using dividers between entries. Align labels, version numbers, and actions consistently. Leave space around the primary decision. Small screens stack related groups and preserve table structure through horizontal scrolling.

## Shape

Controls use an 8 px radius, grouped surfaces 12 px, and larger panels/dialogs 16 px. Pills are reserved for small bounded utility shapes. The rounded-square app icon has its own source geometry; do not force the logo tiles to use control radii.

Standard controls are 42 px tall, compact actions 34 px, and prominent actions 48 px. Use at least 24 px target areas with sufficient separation; use the larger control sizes for touch interaction.

## Icons and artwork

Use the bundled Lucide outlines with an 18 px default box and 14 px status box. Colors inherit from the surrounding text. Use `mask-image` with a local SVG for a theme-aware icon:

```html
<span class="kl-icon" aria-hidden="true"
  style="mask-image:url('/assets/ui-icons/download.svg')"></span>
```

An icon inside a named button is decorative. A meaningful image needs alt text. Do not mix unrelated outline icon families. The K mark, lane pattern, and brand cover live in the asset library; use those artwork files directly.

## Elevation and motion

Ordinary surfaces use grouping, background tone, and a light border. Floating notifications use the floating shadow token; dialogs use the dialog shadow and native modal backdrop. Do not add a shadow to every row.

Use 160 ms for small control changes and 200 ms for larger transitions, with `cubic-bezier(0.2, 0, 0, 1)`. Motion should explain a state change. The system disables animation and transitions under `prefers-reduced-motion`. Keep important information visible without motion.

## Theme semantics

Apply `data-theme` to the document root. Use semantic token roles for UI surfaces and text. Light mode favors pale/white surfaces with navy ink; dark mode uses navy surfaces and light text. Keep logo accent colors unchanged. Bright cyan/mint are never the sole carrier of state information.
