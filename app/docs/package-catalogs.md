# Package-manager catalog matching

Snapshot generated 2026-10-16. Matching runs locally against bundled metadata;
scanning does not send an installed-app list to a service or execute package scripts.

| Source | Coverage in snapshot | Recognition |
| --- | --- | --- |
| WinGet | 15,635 packages; 106,631 unambiguous uninstall codes; 16,691 name/publisher pairs | Exact registry code first, then conservative name/publisher metadata |
| Homebrew Cask | 6,835 active casks; 1,202 unambiguous bundle identifiers | Exact bundle IDs explicitly declared by cask `uninstall`/`quit` metadata |
| Flathub | 3,318 desktop applications | Full installed stable ref and exact app ID corroborated by the catalog ref |

Homebrew casks without an explicit, unambiguous bundle ID remain available as
cross-platform package references but cannot recognize a local bundle by name alone.
Disabled/deprecated casks, Flatpak runtimes, beta refs and conflicting IDs are excluded.
AppStream's unlocalized name is used; legacy `.desktop` component IDs are resolved
through their explicit bundle refs, never by guessing a package ID.

WinGet name matching removes ASCII spacing/punctuation, preserving `+`, `#`, `@`,
digits and channel words. It never strips version numbers or architecture labels.
Publisher matching removes only common trailing legal suffixes. Both normalized
fields must identify exactly one package; the name must also equal the current
canonical package display name, and the publisher must match the
package-ID publisher namespace. Non-ASCII or conflicting metadata abstains. An exact
uninstall code can still associate an app whose publisher field is missing.

`provider-links.json` records explicit package relationships to curated products
and WinGet IDs. Additional Homebrew/Flathub relationships require an exact HTTPS
homepage plus a matching default name, unique and reciprocal in both catalogs. Name-only
matches never establish cross-OS identity. Versioned and prerelease Homebrew casks
are excluded from automatic cross-OS links. The snapshot has 258 groups present in
both Homebrew and Flathub. Release channels and separate forks retain separate IDs;
for example, the community Linux GitHub Desktop fork is not the official product.

Legacy Flatpak snapshots containing only an app ID abstain until rescanned, since
they cannot distinguish stable from beta. New scans preserve the complete ref in
the existing optional native identifier field, retaining wire compatibility.

App details expose package IDs and source-page links. Availability means either
reviewed vendor evidence or a package listing for that OS family. Source evidence
and notes explain which applies. Curated vendor restrictions take precedence;
absence from a manager means unknown, not unavailable. A community package may use
wrappers or differ from the vendor distribution. Architecture, OS release,
distribution, licensing, binary signatures and actual execution are not verified.
The compatibility planner therefore proposes review rather than installation.
An already-recognized counterpart is not proposed for reinstall.

Flatpak remote aliases are observations, not repository authentication. A catalog
association from an exact app ID does not authenticate the installed package or its
remote. The current scanner coverage remains Windows uninstall registry, macOS app
bundles, and user/system Flatpak apps; MSIX, portable apps, native Linux package
inventories and other scan exclusions remain outside these adapters. No package
manager can recognize every program, and unresolved apps stay visible.

## Refresh

Use the existing WinGet importer with Microsoft's official archive:

```powershell
curl.exe -fL https://cdn.winget.microsoft.com/cache/source2.msix -o target/winget.msix
python tools/import-winget-catalog.py --source target/winget.msix --output core/catalog/winget-index.json --generated-on YYYY-MM-DD
```

Download the documented [Homebrew JSON API](https://formulae.brew.sh/docs/api/) and
[Flathub AppStream catalog](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines):

```powershell
curl.exe -fL https://formulae.brew.sh/api/cask.json -o target/cask.json
curl.exe -fL https://dl.flathub.org/repo/appstream/x86_64/appstream.xml.gz -o target/appstream.xml.gz
python tools/import-provider-catalog.py --homebrew target/cask.json --flathub target/appstream.xml.gz --links core/catalog/provider-links.json --output core/catalog/provider-index.json --generated-on YYYY-MM-DD
python -m unittest discover -s tools/tests
cargo test --workspace --locked
```

Importers use Python stdlib only. They bound inputs, reject malformed metadata,
record source SHA-256 digests/dates and replace output atomically after validation.
Runtime validation checks schemas, sources, counts, identifiers, dates, URLs,
duplicate mappings and canonical references. Digests document refresh inputs; they
do not certify publisher identity. The provider evidence date is the catalog
snapshot date, not a claim that every vendor page was manually reviewed.

Catalogs refresh with application builds/releases, not automatically at runtime.
Homebrew package definitions are BSD-2-Clause; WinGet retains its bundled MIT
notice. See [provider attribution](../core/catalog/provider-NOTICE.md).

## Validation (2026-10-16)

- 222 Rust workspace tests passed; five existing opt-in tests remain ignored.
- 17 Python importer tests passed; both indexes regenerated byte-for-byte.
- Formatting and strict workspace Clippy passed. The sandbox emitted incremental
  cache hard-link warnings and used file copies; no code lint failures remained.
- Core compile checks passed for Linux x86_64 and macOS Apple Silicon.
- Read-only Windows diagnostic: 144 entries, 72 catalog matches (51 through WinGet
  identifiers, 10 through name/publisher metadata), three coverage warnings.
- Native app-details preview checked with synthetic inventory and package links.
  Interactive macOS/Linux scans still require testing on those operating systems.
