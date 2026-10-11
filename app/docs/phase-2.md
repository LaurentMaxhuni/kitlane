# Phase 2: cross-platform observations and explicit local sync

Add bounded read-only macOS bundle and Linux Flatpak inventory, catalog associations using observed native identifiers, desktop enrollment with OS credential storage, and review-only cross-device compatibility proposals. Installation/update execution and AI suggestions remain disabled. Native metadata is observation, not verified publisher identity.

Ownership: macOS worker owns `core/src/inventory_macos.rs`; Linux worker owns `core/src/inventory_linux.rs`; catalog/planner worker owns `core/src/catalog.rs`, `core/catalog/`, `core/src/planner.rs`; sync worker owns `sync-client/`. Orchestrator owns shared models, manifests, scanner dispatch, desktop integration, final validation, and docs.

`InstalledApp.native_id` holds an observed bundle/Flatpak identifier; `origin` holds package origin when known. Both optional fields deserialize older snapshots unchanged. No new fields authorize execution. Linux scope and Flatpak origin must remain explicit; community packaging does not become an official release.

Desktop sync is opt-in and targets the existing loopback-only prototype. Pairing credentials go into the OS credential store, never redb or plaintext config. Inventory sharing requires explicit enrollment and a user-triggered sync. Remote networking, hosted accounts, and unattended transport remain future work.
