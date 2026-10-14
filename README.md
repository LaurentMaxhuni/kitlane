# Kitlane

The native Rust app lives in [`app/`](app/README.md), with Cargo managing its dependencies. Run `cargo run --manifest-path app/Cargo.toml` to launch it. The separate marketing website lives in [`website/`](website/README.md), with landing and download pages. Run `npm run dev` inside `website/` and open **http://localhost:4174**.

All shared design files and Node tooling live in [`design-system/`](design-system/README.md): assets, fonts, tokens, component CSS, web helpers, documentation, and the interactive reference. The Rust app reuses assets, token values, and design guidance. CSS and JavaScript support the website and reference, not the app runtime.

## Open the design reference

```powershell
cd design-system
pnpm install
pnpm dev
```

Open **http://localhost:4173**. Run maintenance commands inside `design-system/`:

```powershell
pnpm build:tokens
pnpm build:assets
pnpm check
```

Use assets from `design-system/assets/` and preserve bundled licenses. Map semantic values from `design-system/tokens/tokens.json` to native Rust UI styles when implementing the app. The marketing website can reuse CSS and web helpers. See the [integration guide](design-system/docs/implementation.md).

App and website code live beside `design-system/`. Each keeps its own tooling.
