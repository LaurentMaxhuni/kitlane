# Catalog evidence

Current package-manager matching and refresh instructions are documented in [Package catalogs](package-catalogs.md). The 2026-10-16 expansion adds Homebrew/Flathub associations and conservative WinGet name/publisher matching alongside exact uninstall codes. Package listings establish OS-family availability, with source links and requirements notes; they do not verify binaries, official maintenance or device compatibility. Curated vendor restrictions take precedence over community packages.

The records below document the original vendor-reviewed catalog and earlier snapshots. Historical statements about unmatched macOS/Flatpak apps or provider-only availability are superseded by the package catalog expansion.

| Product | Exact reviewed registry pairs | Native availability |
| --- | --- | --- |
| Visual Studio Code | `Microsoft Visual Studio Code`, `Microsoft Visual Studio Code (User)`; publisher `Microsoft Corporation` | Windows, macOS, Linux available |
| VLC media player | `VLC media player`; publisher `VideoLAN` | Windows, macOS, Linux available |
| Notepad++ | `Notepad++ (64-bit x64)`, `Notepad++ (ARM 64-bit)`, `Notepad++ (32-bit x86)`; publisher `Notepad++ Team` | Windows available; macOS/Linux unavailable |
| Brave | `Brave`; publisher `Brave Software Inc` | Windows, macOS, Linux available |
| Docker Desktop | `Docker Desktop`; publisher `Docker Inc.` | Windows, macOS, Linux available |
| Firefox | `Mozilla Firefox (x64 en-US)`, `Mozilla Firefox (x86 en-US)`; publisher `Mozilla` | Windows, macOS, Linux available |
| Git | `Git`; publisher `The Git Development Community` | Windows, macOS, Linux available |
| OBS Studio | `OBS Studio`; publisher `OBS Project` | Windows, macOS, Linux available |
| Steam | `Steam`; publisher `Valve Corporation` | Windows, macOS, Linux available |
| Google Chrome | `Google Chrome`; publisher `Google LLC` | Windows, macOS, Linux available |
| Microsoft Edge | `Microsoft Edge`; publisher `Microsoft Corporation` | Windows, macOS, Linux available |
| Discord | `Discord`; publisher `Discord Inc.` | Windows, macOS, Linux available |
| Spotify | `Spotify`; publisher `Spotify AB` | Windows, macOS, Linux available |
| Obsidian | `Obsidian`; publisher `Obsidian` | Windows, macOS, Linux available |
| GitHub Desktop | `GitHub Desktop`; publisher `GitHub, Inc.` | Windows, macOS available; official Linux product unavailable |

## Historical WinGet import (2026-10-15)

The build bundles a compact import of Microsoft's [WinGet source index](https://cdn.winget.microsoft.com/cache/source2.msix), with no runtime download, package-manager dependency, executable invocation, installation or source-agreement acceptance. The upstream source has 15,645 package records; only packages with unambiguous, valid product codes are imported. Exact registration identifiers cover MSI GUIDs and named uninstall keys, including historical versions retained upstream. Shared codes across different packages, malformed metadata and unsupported index schemas are excluded or rejected. Name similarity and publisher normalization are not used.

The checked-in [index](../core/catalog/winget-index.json) records its source URL, source update date, generation date and source archive SHA-256: `712fdaf54275f710c7316d253589ebf74102ac9f3ddace2e5de01087e7c36d8e`. This hash records provenance; it is not vendor-signature verification. Microsoft's MIT license is included beside the data and copied into all native release packages.

Provider identities are namespaced as `winget:<PackageIdentifier>`. An explicit `winget_id` alias maps an imported identifier to an existing curated product, preserving cross-device identity and its reviewed availability. App details distinguish curated metadata from WinGet identifier matches. Imported-only products have no reviewed OS claims, including Windows: a community package listing does not prove official native vendor support.

Windows inventory exposes only registration keys recognized by this index; unrecognized keys remain private. Older saved/shared snapshots without identifiers still use curated matching until a rescan. Matching uses a process-wide validated index and binary search, so it does not rebuild or scan the entire provider catalog per app.

Refresh explicitly from the repository root with Python 3.11+ (stdlib only):

```powershell
$catalogSource = Join-Path $env:TEMP 'kitlane-winget-source2.msix'
Invoke-WebRequest 'https://cdn.winget.microsoft.com/cache/source2.msix' -OutFile $catalogSource
python app/tools/import-winget-catalog.py --source $catalogSource --output app/core/catalog/winget-index.json --generated-on 2026-10-15
python -m unittest discover -s app/tools/tests -v
cargo test --manifest-path app/Cargo.toml -p kitlane-core --locked
```

Set the generation date to the actual refresh date. Inspect the resulting source hash and catalog diff before committing. The importer bounds archive/database/output sizes and record counts, rejects unknown schemas, sorts deterministically, removes ambiguous codes after registry case folding, and replaces the output atomically only after successful validation. It never fetches or runs an installer.

## Curated identity evidence

VS Code's [vendor installer](https://github.com/microsoft/vscode/blob/main/build/win32/code.iss) sets its publisher; [build logic](https://github.com/microsoft/vscode/blob/main/build/gulpfile.vscode.win32.ts) appends the literal `(User)` suffix for that installer. Microsoft's [stable product catalog metadata](https://github.com/microsoft/winget-pkgs/blob/master/manifests/m/Microsoft/VisualStudioCode/1.105.0/Microsoft.VisualStudioCode.locale.en-US.yaml) supplies the branded product name and publisher. This does not include Insiders, Code - OSS, VSCodium or Visual Studio IDE.

VLC's [official 3.0 branch installer source](https://github.com/videolan/vlc/blob/3.0.x/extras/package/win32/NSIS/vlc.win32.nsi.in) writes its product name and publisher to uninstall metadata. No shorthand `VLC`, beta suffix, portable package or repack association is inferred.

VLC's [official macOS bundle plist](https://github.com/videolan/vlc/blob/3.0.x/share/Info.plist.in) declares `CFBundleIdentifier` as `org.videolan.vlc`. Only `MacosBundle` plus that literal ID associates with VLC. The capitalized Flatpak ID, missing IDs, altered suffixes and other inventory sources cannot use this rule. Bundle metadata remains forgeable and no signature check occurs.

VS Code macOS remains unknown pending reviewed evidence for Microsoft's stable branded bundle ID. The [current public product configuration](https://github.com/microsoft/vscode/blob/main/product.json) declares `com.visualstudio.code.oss` for Code - OSS, a separate distribution excluded from the stable product. User reports containing `com.microsoft.VSCode` do not substitute for vendor source or documentation. Stable, Insiders and OSS IDs therefore have no native association in this catalog.

Flatpak associations remain unknown. [Flatpak's official documentation](https://docs.flatpak.org/en/latest/using-flatpak.html#add-a-remote) explains that `flathub` is a local remote name. An inventory origin string containing that alias does not prove its repository URL, signing key, vendor relationship or package authenticity. No ID-plus-alias rule is accepted; metadata alone cannot supply a trusted installation source.

Notepad++'s [vendor registry-writing function](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/installer/nsisInclude/tools.nsh) declares all three architecture display names and publisher. [Global definitions](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/installer/nsisInclude/globalDef.nsh) define `APPNAME` as `Notepad++`. Architecture strings are explicit aliases, not parsed into target architecture. Legacy plain names and separate MSI publisher/name variants remain unknown until separately reviewed.

## Platform evidence

### Expansion reviewed 2026-10-15

The new Windows rules use the exact names and publishers in Microsoft's WinGet catalog, corroborated against local uninstall metadata for Brave, Docker Desktop, Git, Firefox, OBS Studio and Steam. Versioned manifest pages are linked in each product's evidence; these are metadata associations, not signature or package-origin verification. WinGet's display labels do not authorize suffix stripping or other inferred aliases.

Git's [installer source](https://github.com/git-for-windows/build-extra/blob/main/installer/install.iss) explicitly sets its uninstall name and publisher. GitHub Desktop's [application metadata](https://github.com/desktop/desktop/blob/development/app/package.json) declares its product and company names; the separate MSI deployment tool is deliberately excluded. Firefox's [registry-writing source](https://github.com/mozilla-firefox/firefox/blob/main/browser/installer/windows/nsis/shared.nsh) and [official branding](https://github.com/mozilla-firefox/firefox/blob/main/browser/branding/official/branding.nsi) establish the literal architecture/locale form. Only the two listed English variants are reviewed; other locales, ARM, ESR and prerelease names remain unknown. No version is encoded in these rules.

Official availability sources: [Brave](https://brave.com/download/), [Docker Desktop](https://docs.docker.com/desktop/), [Firefox requirements](https://www.mozilla.org/en-US/firefox/157.0.1/system-requirements/), [Git](https://git-scm.com/install/), [OBS](https://obsproject.com/download), [Steam](https://store.steampowered.com/about/), [Chrome requirements](https://support.google.com/chrome/answer/95346), [Edge](https://www.microsoft.com/en-us/edge/download), [Discord](https://discord.com/download), [Spotify](https://www.spotify.com/us/download/other/) ([Linux caveats](https://www.spotify.com/us/download/linux/)), [Obsidian](https://obsidian.md/download), and [GitHub Desktop requirements](https://docs.github.com/en/desktop/overview/supported-operating-systems-for-github-desktop). These establish OS-family availability only. Docker virtualization/licensing, OBS graphics requirements, Linux distribution support and release-specific constraints are not evaluated. Steam client availability says nothing about individual games. GitHub's [README](https://github.com/desktop/desktop/blob/development/README.md) explicitly excludes official Linux support and identifies a separate community fork; that fork is not matched to the official product.

### Original entries reviewed 2026-10-11

Microsoft's [requirements](https://code.visualstudio.com/docs/supporting/requirements) explicitly document Windows client, macOS and Linux support. Supported OS releases, 64-bit Windows and Linux native libraries constrain installation; this catalog does not evaluate those constraints on a device.

VideoLAN's official [Windows page](https://www.videolan.org/vlc/download-windows.html) lists desktop architecture variants; [macOS page](https://www.videolan.org/vlc/download-macosx.html) documents Intel/Apple silicon and a minimum OS; [Ubuntu page](https://www.videolan.org/vlc/download-ubuntu.html) establishes Linux availability. Ubuntu availability does not imply all Linux distributions, architectures or third-party packaging are supported. Browser fetches returned HTTP 418; direct HTTPS reads of the same vendor pages succeeded during review.

Notepad++'s [supported-system table](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/SUPPORTED_SYSTEM.md) establishes Windows availability and distinguishes releases that can run from releases receiving vendor support. Its [official FAQ](https://github.com/notepad-plus-plus/notepad-plus-plus/wiki/FAQ) explicitly says the product targets Windows only with no other-system port planned. This affirmative restriction supports unavailable native macOS/Linux status. Wine, VMs and forks are separate relationships; an absent package alone would not support that claim.

Evidence URLs point to documentation or source pages, never executable artifacts. Review dates record inspection, not future validity guarantees. Moving upstream branches can change after this review; later catalog updates need fresh review.

## Validation

Schema version, catalog byte/product/rule/evidence limits, duplicate product IDs, duplicate name/publisher pairs, duplicate native identifiers, exact three-platform ordering, bounded text, HTTPS page URLs and calendar-valid review dates are checked before use. Schema v1 catalogs without the optional `native_match_rules` field remain accepted; their Windows associations retain identical semantics. Native rules currently permit only reviewed macOS bundle IDs. Available/unavailable claims and every identity rule require evidence; unknown support can remain without evidence. Unexpected fields are rejected. Source truth remains a human review responsibility: structural checks cannot prove a vendor claim.

Unit tests cover reviewed pairs and bundle IDs, publisher mismatch/missing publisher, missing/wrong native IDs, cross-source ID collisions, unreviewed editions/forks/aliases, unsupported inventory sources, malformed observations, ambiguous rules, backward catalogs, duplicate products/platforms, unsupported schema, missing evidence, URL schemes/credentials, invalid dates, unknown fields and byte limits.

Expansion regression fixtures cover all twelve added products, mismatched publishers, channel suffixes, Firefox maintenance/ESR, Brave Nightly, Discord Canary, Edge WebView2 and GitHub's deployment tool. Official GitHub Desktop Linux unavailability is asserted separately.

Windows validation, 2026-10-15: 136 workspace tests passed, two opt-in tests ignored; strict workspace Clippy and formatting passed. The same read-only native inventory diagnostic reported 144 applications and three coverage warnings before and after expansion; catalog matches increased from one to eleven. Run native diagnostics outside the restricted sandbox, which hides parts of the user registry; account/sync tests also need loopback sockets unavailable inside that sandbox.

WinGet follow-up validation, 2026-10-15: 141 workspace tests passed, two opt-in tests ignored; six importer tests passed. Strict workspace Clippy, formatting and packaging-script syntax checks passed. Regeneration from the same archive/date was byte-for-byte identical. Malformed schema/database, duplicate package IDs, invalid dates and ambiguous codes are covered; a failed refresh leaves existing output intact. Independent code review completed with its importer validation finding fixed. The same native diagnostic found 60 catalog matches among 144 applications (49 via WinGet identifiers), with the same three coverage warnings. Native installer builds and interactive macOS/Linux checks were not rerun for this change.
