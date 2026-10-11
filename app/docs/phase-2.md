# Phase 2: cross-platform observations and explicit local sync

Add bounded read-only macOS bundle and Linux Flatpak inventory, catalog associations using observed native identifiers, desktop enrollment with OS credential storage, and review-only cross-device compatibility proposals. Installation/update execution and AI suggestions remain disabled. Native metadata is observation, not verified publisher identity.

Ownership: macOS worker owns `core/src/inventory_macos.rs`; Linux worker owns `core/src/inventory_linux.rs`; catalog/planner worker owns `core/src/catalog.rs`, `core/catalog/`, `core/src/planner.rs`; sync worker owns `sync-client/`; auth worker owns server credential recovery and enrollment checks. Orchestrator owns shared models, manifests, scanner dispatch, desktop integration, final validation, and docs. All agents use `gpt-6.1-sol` at medium/high effort in isolated implementation worktrees, with a separate source reviewer.

`InstalledApp.native_id` holds an observed bundle/Flatpak identifier; `origin` holds package origin when known. Both optional fields deserialize older snapshots unchanged. No new fields authorize execution. Linux scope and Flatpak origin must remain explicit; community packaging does not become an official release.

Desktop sync is opt-in and targets the existing loopback-only prototype. Pairing credentials go into the OS credential store, never redb or plaintext config. Inventory sharing requires explicit enrollment and a user-triggered sync. Remote networking, hosted accounts, and unattended transport remain future work.

## Integrated behavior

The platform dispatcher selects Windows registry, macOS bundle, or Linux Flatpak inventory. Adapters preserve scope and observed identifiers; the UI labels Flatpak origins unverified. Unknown products remain unknown. Reviewed VLC bundle metadata can associate the macOS observation with its catalog product; VS Code macOS and Flatpak observations remain unknown where evidence is insufficient.

Devices provides identity copying, explicit sharing consent, enrollment, saved-credential checking, manual synchronization, and credential removal. Endpoint changes reset consent and clear peer plans. Compatibility proposals use peer snapshots and separate already-present, unavailable, unknown, and review-required outcomes. None execute installers or updates. See [client behavior](../sync-client/README.md) and [proposal rules](compatibility-plans.md).

Enrollment supplies expected device metadata before grant consumption. Wrong-device grants fail without consumption; returned metadata is still checked. Owner-issued recovery grants rotate lost credentials under the same identity while preserving inventory/revision, reject stale/revoked grants, and invalidate old tokens. See [service contract](../sync-server/README.md).

## Validation

Windows, 2026-10-11: 84 workspace tests passed; two opt-in tests skipped in the default run. The native credential-store test passed separately, creating/removing only one unique test entry. The read-only Windows diagnostic found 143 applications, one catalog match, and three coverage warnings. Strict workspace Clippy and formatting passed. Core Clippy compiled all targets/tests for Linux x86-64 and macOS ARM64. Fixtures cover malformed/oversized metadata, binary plist reference expansion, enrollment/recovery, wrong grants, lost responses, revision conflicts, and a real two-device Rust service/client exchange using an injected vault.

Final release opened, created/reopened isolated local state, and closed cleanly twice. Executable measured 9.11 MiB with approximately 85 MiB startup working set. These single-device observations do not establish a cross-platform performance benchmark.

The native Windows/macOS/Linux CI workflow is configured but has not run remotely. macOS/Linux runtime scans, Unix link/FIFO behavior, and native vault access require those systems. Local code-only architecture index lives in ignored `app/graphify-out/graph.json`; source and tests remain authoritative.

Next: reviewed remote transport/account isolation and provider-specific install/update plans, validated in disposable machines before enabling execution. AI alternatives follow verified catalog coverage and explicit acceptance.
