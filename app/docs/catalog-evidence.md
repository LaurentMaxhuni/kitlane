# Catalog evidence

Reviewed 2026-10-11. Bundled metadata only; no runtime network, installer URLs, package commands, provider IDs or automatic installation. Canonical IDs are internal Kitlane IDs.

A match means **catalog association from source metadata**. Registry display names, publishers and bundle identifiers can be forged. Matching does not verify binary publisher identity, package origin, executable signatures or safe-install authorization. Availability means an official native product exists for the OS family. It does not establish compatibility with a device's architecture, OS version, distribution, license or deployment environment.

`WindowsRegistry` observations require exact, case-sensitive display-name and publisher pairs. `MacosBundle` observations require an exact reviewed bundle identifier; display names can vary and do not establish identity. Version, scope, registry key and package origin do not establish identity. Manual entries, Flatpak metadata, missing required identifiers and unreviewed metadata remain unknown. No punctuation stripping, case folding, substring matching or generic suffix removal. Identity unknown never means Windows-only.

| Product | Exact reviewed registry pairs | Native availability |
| --- | --- | --- |
| Visual Studio Code | `Microsoft Visual Studio Code`, `Microsoft Visual Studio Code (User)`; publisher `Microsoft Corporation` | Windows, macOS, Linux available |
| VLC media player | `VLC media player`; publisher `VideoLAN` | Windows, macOS, Linux available |
| Notepad++ | `Notepad++ (64-bit x64)`, `Notepad++ (ARM 64-bit)`, `Notepad++ (32-bit x86)`; publisher `Notepad++ Team` | Windows available; macOS/Linux unavailable |

## Identity evidence

VS Code's [vendor installer](https://github.com/microsoft/vscode/blob/main/build/win32/code.iss) sets its publisher; [build logic](https://github.com/microsoft/vscode/blob/main/build/gulpfile.vscode.win32.ts) appends the literal `(User)` suffix for that installer. Microsoft's [stable product catalog metadata](https://github.com/microsoft/winget-pkgs/blob/master/manifests/m/Microsoft/VisualStudioCode/1.105.0/Microsoft.VisualStudioCode.locale.en-US.yaml) supplies the branded product name and publisher. This does not include Insiders, Code - OSS, VSCodium or Visual Studio IDE.

VLC's [official 3.0 branch installer source](https://github.com/videolan/vlc/blob/3.0.x/extras/package/win32/NSIS/vlc.win32.nsi.in) writes its product name and publisher to uninstall metadata. No shorthand `VLC`, beta suffix, portable package or repack association is inferred.

VLC's [official macOS bundle plist](https://github.com/videolan/vlc/blob/3.0.x/share/Info.plist.in) declares `CFBundleIdentifier` as `org.videolan.vlc`. Only `MacosBundle` plus that literal ID associates with VLC. The capitalized Flatpak ID, missing IDs, altered suffixes and other inventory sources cannot use this rule. Bundle metadata remains forgeable and no signature check occurs.

VS Code macOS remains unknown pending reviewed evidence for Microsoft's stable branded bundle ID. The [current public product configuration](https://github.com/microsoft/vscode/blob/main/product.json) declares `com.visualstudio.code.oss` for Code - OSS, a separate distribution excluded from the stable product. User reports containing `com.microsoft.VSCode` do not substitute for vendor source or documentation. Stable, Insiders and OSS IDs therefore have no native association in this catalog.

Flatpak associations remain unknown. [Flatpak's official documentation](https://docs.flatpak.org/en/latest/using-flatpak.html#add-a-remote) explains that `flathub` is a local remote name. An inventory origin string containing that alias does not prove its repository URL, signing key, vendor relationship or package authenticity. No ID-plus-alias rule is accepted; metadata alone cannot supply a trusted installation source.

Notepad++'s [vendor registry-writing function](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/installer/nsisInclude/tools.nsh) declares all three architecture display names and publisher. [Global definitions](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/installer/nsisInclude/globalDef.nsh) define `APPNAME` as `Notepad++`. Architecture strings are explicit aliases, not parsed into target architecture. Legacy plain names and separate MSI publisher/name variants remain unknown until separately reviewed.

## Platform evidence

Microsoft's [requirements](https://code.visualstudio.com/docs/supporting/requirements) explicitly document Windows client, macOS and Linux support. Supported OS releases, 64-bit Windows and Linux native libraries constrain installation; this catalog does not evaluate those constraints on a device.

VideoLAN's official [Windows page](https://www.videolan.org/vlc/download-windows.html) lists desktop architecture variants; [macOS page](https://www.videolan.org/vlc/download-macosx.html) documents Intel/Apple silicon and a minimum OS; [Ubuntu page](https://www.videolan.org/vlc/download-ubuntu.html) establishes Linux availability. Ubuntu availability does not imply all Linux distributions, architectures or third-party packaging are supported. Browser fetches returned HTTP 418; direct HTTPS reads of the same vendor pages succeeded during review.

Notepad++'s [supported-system table](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/SUPPORTED_SYSTEM.md) establishes Windows availability and distinguishes releases that can run from releases receiving vendor support. Its [official FAQ](https://github.com/notepad-plus-plus/notepad-plus-plus/wiki/FAQ) explicitly says the product targets Windows only with no other-system port planned. This affirmative restriction supports unavailable native macOS/Linux status. Wine, VMs and forks are separate relationships; an absent package alone would not support that claim.

Evidence URLs point to documentation or source pages, never executable artifacts. Review dates record inspection, not future validity guarantees. Moving upstream branches can change after this review; later catalog updates need fresh review.

## Validation

Schema version, catalog byte/product/rule/evidence limits, duplicate product IDs, duplicate name/publisher pairs, duplicate native identifiers, exact three-platform ordering, bounded text, HTTPS page URLs and calendar-valid review dates are checked before use. Schema v1 catalogs without the optional `native_match_rules` field remain accepted; their Windows associations retain identical semantics. Native rules currently permit only reviewed macOS bundle IDs. Available/unavailable claims and every identity rule require evidence; unknown support can remain without evidence. Unexpected fields are rejected. Source truth remains a human review responsibility: structural checks cannot prove a vendor claim.

Unit tests cover reviewed pairs and bundle IDs, publisher mismatch/missing publisher, missing/wrong native IDs, cross-source ID collisions, unreviewed editions/forks/aliases, unsupported inventory sources, malformed observations, ambiguous rules, backward catalogs, duplicate products/platforms, unsupported schema, missing evidence, URL schemes/credentials, invalid dates, unknown fields and byte limits.
