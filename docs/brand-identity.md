# Kitlane Brand System

**Version:** 0.1  
**Status:** Initial brand direction  
**Category:** Cross-platform desktop software setup manager

## Brand core

**Purpose:** Help people keep a trusted, useful software setup across their computers.

**Promise:** One software profile, adapted to each computer.

**Tagline:** Your tools, ready on every machine.

**Audience:** People who use more than one computer or operating system and want their everyday tools to be easy to find, install, update, and understand.

**Personality:** Capable, calm, clear, trustworthy, and a little playful. Kitlane explains availability and source quality plainly. It never implies that every app works everywhere or that an installer is perfectly safe.

**Brand idea:** A personal toolkit travels along a lane. The modular K mark combines app tiles with a path that connects them.

## Typography

The exact typeface in the supplied raster logo cannot be confirmed from the image; it may be custom or generated lettering. **Sora ExtraBold 800** is Kitlane’s chosen display and wordmark match. It has the geometric, rounded weight that makes the reference appealing.

| Role | Typeface | Weights | Use |
| --- | --- | --- | --- |
| Wordmark and display | Sora | 800, 700 | Logo lettering, hero headlines, major page titles |
| UI and body | Inter | 400, 500, 600 | App lists, settings, descriptions, controls, tables |
| Technical values | System monospace stack | 400, 500 | Version numbers, package IDs, commands, checksums |

Use sentence case and comfortable line spacing. Avoid setting long paragraphs in Sora ExtraBold. When recreating the wordmark, start with Sora ExtraBold 800 and adjust kerning optically; do not stretch the letters. Use the included source-traced vector logo as the current identity master; the supplied raster remains the visual reference.

## Color

The core values below are sampled from the supplied logo and icon. Use navy for text and structure; cyan and mint are bright accents.

| Token | Hex | Role |
| --- | --- | --- |
| Ink / Night | `#081B31` | Primary text, app icon background, dark surfaces |
| Brand Navy | `#091C31` | Logo mark and wordmark on light backgrounds |
| Route Cyan | `#01BCFB` | Active selections, focus details, route tile |
| Sync Mint | `#19F5BE` | Synced/healthy state, route tile |
| White | `#FFFFFF` | Main surface and reversed logo |
| Pale | `#F4F7FA` | Quiet app background and grouped surfaces |
| Slate | `#607286` | Secondary text and low-emphasis icons |
| Border | `#DCE5EC` | Dividers and control outlines |

### Color use

- Use a white or pale background for the default app experience, with navy text.
- Use navy for primary buttons with white labels. Reserve cyan for the selected state, focus ring, and key route accents.
- Use mint for a synced or healthy indicator, paired with a label or icon.
- Do not use cyan or mint as small text on white. Use navy text on cyan or mint fills.
- In dark mode, use Ink / Night as the base, a slightly lighter navy for raised surfaces, white for primary text, and slate for secondary text.
- Add amber and red only for warning and error states. Status must never rely on color alone.

## Logo system

The supplied modular K and wordmark are the approved direction.

- **Primary lockup:** navy mark and wordmark with cyan and mint tiles on white or pale.
- **Reversed lockup:** white mark/wordmark on navy; keep the cyan and mint tiles.
- **App icon:** the modular K centered on the navy rounded-square field. Preserve generous inset space so operating-system masks do not crop the mark.
- **Clear space:** keep at least one tile-width of open space around the complete lockup.
- **Small use:** use the symbol alone in compact navigation and app icons. Use the full lockup when the name needs to be learned.
- Keep proportions and tile colors consistent. Do not skew, rotate, outline, add a gradient, add a glow, or place the mark over a busy image.

The original raster remains in assets/source/. Source-traced SVG and PNG variants, desktop icon containers, and light/dark tray icons are included in assets/. The trace preserves the original lettering and smooths pixel stair steps. Inspect the smallest tray sizes in the actual desktop shell before release.

## Interface language

**Default mode:** light, clean, and spacious. The brand should feel like a dependable tool, not a monitoring console.

- Build layouts on a 4 px spacing rhythm: `4, 8, 12, 16, 24, 32, 48, 64`.
- Use 8 px control corners, 12 px grouped surfaces, and 16 px larger panels. Keep app-icon rounding platform appropriate.
- Prefer grouped rows, alignment, and dividers over a card for every app.
- Use a consistent 1.75 px outline icon style with rounded joins. Icons clarify status or action; they do not decorate empty space.
- Use restrained motion around 160–200 ms for state changes. Respect reduced-motion preferences and avoid pulsing or glowing effects.

### Core status language

| State | Example label | Treatment |
| --- | --- | --- |
| Installed | `Installed` | Check icon + concise label |
| Update available | `Update available` | Cyan action cue + version details |
| Synced | `Synced` | Mint cue + last-sync detail when useful |
| Unsupported | `Unsupported on this OS` | Neutral treatment + clear explanation |
| Alternative | `Closest verified match` | Explain why it matches and where it differs |
| Unverified source | `Source not verified` | Warning treatment; require review before install |

Make the source, publisher, version, and update policy easy to inspect. Never imply that an alternative is the same app. Do not silently replace an unsupported app.

## Voice and copy

Write short, specific, plain-language messages. Explain what is available on this computer, where an installer comes from, and what will happen before an update.

**Use:** “Available for Windows.” “This app has no verified Linux build.” “Closest match: [app]. Supports [format]. Known differences: [details].”

**Avoid:** “Works everywhere.” “100% safe.” “Magic sync.” “Perfect replacement.”

## Brand applications

- **Product:** software library, device status, install and update flows, portability matches.
- **Pattern:** repeat the modular tiles and lane sparingly as a divider or quiet background texture.
- **Marketing:** show a clear app-availability example and explain the match; avoid generic cloud imagery.
- **Iconography:** use the K mark as the main identifying symbol. Use standard interface icons for platform, install, update, and warning actions.

## Implementation handoff

Use tokens/tokens.json as the editable source and styles/index.css as the importable token and component layer. Keep the logo asset separate from live text: the logo is artwork, while headings and interface text use the type stacks above. Verify color contrast and logo legibility at actual desktop and tray-icon sizes before release.




