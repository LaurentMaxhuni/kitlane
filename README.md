# Kitlane

Shared design foundations for the future Kitlane app and marketing website.

All design-system files live in [`design-system/`](design-system/README.md): brand assets, fonts, tokens, component CSS, DOM helpers, documentation, and the interactive reference. Both frontends should reuse this package so brand and interface changes have one source of truth.

## Open the design reference

```powershell
pnpm install
pnpm dev
```

Open **http://localhost:4173**. Root commands delegate to `@kitlane/design-system`:

```powershell
pnpm build:tokens
pnpm build:assets
pnpm check
```

## Use in the app or marketing website

When adding either frontend to this workspace, declare `"@kitlane/design-system": "workspace:*"` in its dependencies. Import shared styles and helpers through the package exports:

```css
@import '@kitlane/design-system/styles';
```

```js
import { setTheme, initTabs, toast } from '@kitlane/design-system';
```

Use assets from `design-system/assets/` and preserve bundled licenses. See the [integration guide](design-system/docs/implementation.md) for themes, markup, and font paths. App and website code can live beside `design-system/` when implementation begins.
