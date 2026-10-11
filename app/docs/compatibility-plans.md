# Compatibility plans

`build_plan(catalog, target, peers)` compares inventory metadata locally. It returns review entries and performs no network lookup, package selection, download, installation or AI ranking. Exact catalog associations and OS-family availability drive every decision in code.

Candidates come from peer inventories. Target-only applications are outside the migration plan. A peer with target's device UUID is excluded after validation; conflicting metadata under that UUID fails the request. Known products deduplicate by canonical product ID across peers. Unknown observations remain separate by peer UUID and inventory app ID, even when names or native IDs collide. Results sort by canonical product ID, then unknown observations by device/app ID; peer order does not alter the result.

| Status | Meaning |
| --- | --- |
| Already present | A reviewed association for the same canonical product exists on target. No reinstall is proposed. |
| Review required | Reviewed evidence says an official native product exists for target's OS family. CPU architecture, OS version, distribution, license and trusted package mapping still require human review. |
| Unavailable | Reviewed evidence affirmatively establishes no official native counterpart for target's OS family. |
| Unknown | Source identity or target OS availability lacks a reviewed association/evidence. Absence never becomes an unavailable claim. |

Unknown target identities may conceal an already-installed counterpart. Review-required reasons explicitly call out possible duplicates when such records exist. Names are never compared fuzzily, and signatures are never verified by metadata matching. Source/platform mismatches remain unknown. Catalog associations do not authorize installation.

`PlanEntry.source_devices` contains sorted unique peer device UUIDs, not display names. UUIDs retain provenance when names collide or change; UI resolves each UUID through its peer metadata. Duplicate snapshots with identical device metadata contribute a deterministic union of observations. Conflicting device name, OS family or architecture for the same UUID fails validation. Inventory validation rejects malformed IDs/text and duplicate app IDs inside a snapshot.

Bounds: at most 64 peer snapshots, 50,000 combined input app records including target, and 10,000 output candidates. Per-snapshot inventory limits also apply. Invalid or oversized input returns `PlanError`; no partial plan is returned.

Tests cover multiple peers, duplicate product installations, target identity exclusion, existing cross-platform bundle associations, same-name unknown apps, possible duplicates, absent peers, unavailable versus missing evidence, invalid snapshots, conflicting device metadata, mismatched inventory sources, untrusted Flatpak aliases and all three plan bounds.
