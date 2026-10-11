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
