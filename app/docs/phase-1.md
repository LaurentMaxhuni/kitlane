# Phase 1: inventory and verified availability

First milestone: native Rust UI scans installed Windows applications, saves its device identity and inventory locally, and displays evidence-backed availability for a small reviewed product catalog. Unknown identities and unavailable scan sources remain explicit. No installation or update commands execute in this phase.

The `kitlane-core` library owns shared serialized types in `model.rs`. Agents use isolated worktrees from the same baseline. The orchestrator owns shared manifests, exports, UI integration, final testing, and merge order.

| Track | Owned paths | Boundary |
| --- | --- | --- |
| Database | `core/src/store.rs` | redb persistence, schema checks, device/snapshot/settings APIs |
| Inventory | `core/src/inventory.rs` | read-only Windows registry scan and explicit unsupported-platform result |
| Catalog | `core/src/catalog.rs`, `core/catalog/`, catalog evidence doc | conservative name/publisher matching, compatibility facts, unknown outcome |
| Auth/sync | `sync-server/` | standalone Rust service, secure device enrollment/authentication, bounded snapshot sync; prototype locally before deployment |
| Orchestrator | shared models/manifests, desktop GUI, integration docs | combine worker results, validate native milestone, review service boundaries |

Cloud provider/account login, automatic install/update execution, native macOS/Linux scanning, and AI suggestions follow later phases. The service prototype must not be presented as a deployed account system or silently upload local inventory. The first desktop milestone remains local unless explicit enrollment is configured.

## Integrated milestone

The desktop now scans Windows registry inventory on a dedicated worker, persists device identity, snapshots, and appearance in redb, renders searchable virtualized results, and shows reviewed vendor evidence for VS Code, VLC, and Notepad++. Unmatched apps stay unknown. See [inventory coverage](windows-inventory.md) and [catalog evidence](catalog-evidence.md).

The [local sync prototype](../sync-server/README.md) provides durable enrollment, single-use pairing grants, device-bound revisioned snapshots, revocation, and bounded requests/database work. It runs independently on loopback; desktop enrollment and remote transport remain future work.

All implementation agents used `gpt-6.1-sol` at medium or high effort in separate worktrees. Shared contracts, GUI integration, final verification, and merge decisions stayed with the orchestrator.

Validation on Windows, 2026-10-11: 34 workspace tests passed; the opt-in live registry test was run separately and passed. Strict workspace Clippy and formatting passed. Read-only diagnostic observed 143 applications, one catalog match, and three coverage warnings. Native release startup, isolated database creation, and clean window close passed. Executable measured 7.81 MiB with an approximately 85 MiB startup working set; this single-device observation is not a cross-platform performance benchmark.
