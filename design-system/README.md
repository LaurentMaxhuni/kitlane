# Kitlane design system

Kitlane's brand identity, local fonts, vector logos, desktop icons, semantic design tokens, component styles, and interactive reference. The pure Rust app shares assets, tokens, and design guidance; CSS and DOM helpers serve the marketing website and reference.

## Open the reference

Run these commands from `design-system/`:

```powershell
pnpm install
pnpm dev
```

Open **http://localhost:4173**. The reference includes a theme switch, font weight explorer, copyable swatches, buttons, form controls, statuses, tabs, dialog, toast, progress, app list, empty state, and connected-computer pattern. Examples describe interface behavior and do not represent a live software catalog.

Node.js 22 or newer is recommended. The reference server has no runtime package dependencies. Fonts and UI assets are local, so the reference works without network access after checkout and installation.

## Contents

| Location | Contents |
| --- | --- |
| `assets/logos/` | Primary, reversed, monochrome, symbol, and wordmark SVG/PNG assets |
| `assets/icons/` | App icon in SVG and PNG, Windows ICO, macOS ICNS, dark/light tray icons |
| `assets/fonts/` | Sora and Inter variable fonts, Inter italic, and their OFL licenses |
| `assets/ui-icons/` | A focused selection of Lucide outline icons and its license |
| `assets/source/` | The original approved raster artwork and visual brand board |
| `tokens/tokens.json` | Typed foundation tokens and semantic theme aliases |
| `styles/` | Generated tokens, local font declarations, and reusable component CSS |
| `src/index.js` | Theme, accessible tabs, and dismissible toast helpers |
| `docs/` | Identity, component usage, accessibility, and implementation guidance |
| `index.html`, `demo/` | The browsable reference and its example interactions |
| `scripts/` | Local server, asset exports, token generation, and integrity checks |

## Use in the marketing website

```css
@import './path/to/design-system/styles/index.css';
```

Add `kl-root` to your website's top-level element and set `data-theme="light"` or `data-theme="dark"` on the document root. Use semantic variables such as `var(--kl-text)`, `var(--kl-surface)`, and `var(--kl-border)` for interface styling.

```html
<main class="kl-root">
  <button class="kl-button">Install app</button>
  <span class="kl-badge kl-badge--success">Installed</span>
</main>
```

```js
import { setTheme, initTabs, toast } from './path/to/design-system/src/index.js';
const cleanup = setTheme('system');
initTabs(document.getElementById('platform-tabs'));
toast('Your app profile is synced.');
// Call cleanup() when the theme controller is replaced or unmounted.
```

Keep CSS and asset paths together, or adjust font URLs when copying into the website's public assets. The Rust app uses assets and token values through its native implementation. See [Implementation](docs/implementation.md) for integration details.

## Update the system

```powershell
pnpm build:tokens # Regenerate styles/tokens.css after token changes
pnpm build:assets # Re-export vectors, PNGs, desktop icons, and UI icons
pnpm check        # Check references, fonts, icon containers, tokens, and contrast
```

The vector exports trace the supplied artwork's actual shapes, including its wordmark. **Sora ExtraBold 800** is the chosen matching live-text font; the original raster's exact font cannot be identified with certainty. Inter is the interface font. The source artwork remains available in `assets/source/`.

Font provenance: [Sora](https://github.com/google/fonts/tree/main/ofl/sora) and [Inter](https://github.com/google/fonts/tree/main/ofl/inter) from the Google Fonts repository. Icon provenance: [Lucide](https://lucide.dev/). Their licenses are bundled alongside their files.

Start with [Brand identity](docs/brand-identity.md), [Visual foundations](docs/foundations.md), [Components](docs/components.md), and [Accessibility](docs/accessibility.md).
