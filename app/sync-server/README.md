# Kitlane local sync prototype

Rust service foundation for one self-hosted owner and explicitly enrolled devices. Binds **127.0.0.1 only**. No account signup, password authentication, cloud identity provider, multiaccount hosting, automatic desktop upload, installation commands, or remote execution. The current desktop does not call this service. This is an inventory sync prototype, not a deployed account system.

## Run

Requires Rust 1.95. From `app/`:

```powershell
# Provision once; securely retain this value for subsequent starts. Do not print it.
$env:KITLANE_OWNER_TOKEN = [Convert]::ToHexString([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32)).ToLowerInvariant()
$env:KITLANE_SYNC_DB = 'C:\private-kitlane\sync.redb'
$env:KITLANE_SYNC_PORT = '8787'
cargo run --manifest-path sync-server/Cargo.toml
```

Create the private database directory first. Restrict its Windows ACL to the service owner; Unix initial database files use mode 0600. Configuration requires exactly 32 random bytes encoded as 64 hex characters; format validation cannot prove entropy, so generate with an OS cryptographic RNG. Never use a human password. The owner token belongs to service administration, not the desktop binary. Environment configuration is a bootstrap mechanism, not a credential vault; deployment-specific secret storage is deferred.

Default database is `kitlane-sync.redb` in the current directory; default port is 8787. There is deliberately no bind-address setting or LAN mode. HTTP is plaintext on loopback. Do not expose or port-forward the listener. Connecting separate computers requires a future reviewed transport/deployment with TLS, access controls, resource limits, and an identity-provider choice; this prototype does not configure that infrastructure.

The owner hash is pinned in the database. A different configured token rejects startup; owner credential rotation/recovery has no API yet. Keep a protected backup of configuration and database. Initialization builds a complete sibling file, syncs it, and publishes via an exclusive hard link. Existing files are never replaced or adopted; incompatible, schema-less, missing-table, and unrelated files fail closed. The filesystem must support hard links. A process crash can leave a `.kitlane-sync-init-*.redb` orphan; the final path remains absent or fully initialized. Such orphans contain sensitive inventory/credential hashes and may be removed manually after stopping the service. They are never loaded on restart.

## HTTP contract

All bodies/responses use JSON with snake_case fields. Unknown request fields are rejected. Protected endpoints require exactly one `Authorization: Bearer <64-hex-character-token>` header. Never send tokens in URLs. Every response sets `Cache-Control: no-store`; no CORS or cookie authentication is enabled. Responses and request bodies must not be logged by a future client/proxy.

| Endpoint | Credential | Request | Success |
| --- | --- | --- | --- |
| `POST /v1/pairing-grants` | Owner only | `{"device": DeviceInfo}` | 200 `{"pairing_token": string, "expires_at_unix_secs": integer, "device": DeviceInfo}` |
| `POST /v1/devices/{id}/pairing-grants` | Owner only | Empty/no body | 200 same pairing-grant response; recover an active device credential |
| `POST /v1/enroll` | Single-use initial/recovery grant in body | `{"pairing_token": string}` | 201 `{"device_token": string, "device": DeviceInfo}` |
| `GET /v1/devices` | Owner or active device | None | 200 array of `{"device": DeviceInfo, "revision": integer}` |
| `DELETE /v1/devices/{id}` | Owner only | None | 204; revoke token, remove current snapshot, omit device from roster |
| `PUT /v1/snapshots/{id}` | That active device only | `{"revision": integer, "snapshot": InventorySnapshot}` | 204 |
| `GET /v1/snapshots/{id}` | Owner or active device | None | 200 `{"revision": integer, "snapshot": InventorySnapshot}` |

`DeviceInfo` and `InventorySnapshot` are shared `kitlane-core::model` types. Device identity must be a non-nil canonical lowercase UUID. Owner-issued pairing fixes device ID, name, platform, and architecture; snapshots must match all enrolled metadata. Metadata change requires a new identity in this prototype.

```json
{
  "device": {
    "id": "9c1f7c60-e081-4e69-b073-7f82d2c925fb",
    "name": "Workstation",
    "platform": "windows",
    "architecture": "x86_64"
  }
}
```

Pairing grants contain 256 bits of OS randomness, expire after 300 seconds, and enroll exactly once even under concurrent requests. An enrolled device token also contains 256 random bits; only its SHA-256 hash is persisted. Hash comparisons use `subtle` constant-time equality. Grants, device credentials, tombstones, revisions, and snapshots persist transactionally in redb. The owner must transfer the one-time grant through a trusted channel; there is no short numeric pairing code or unauthenticated grant issuance. A grant holder is authorized to enroll the identity fixed by that grant.

An active enrolled device can recover a lost token or failed credential-store write through owner-authorized `POST /v1/devices/{id}/pairing-grants` with an empty body. The owner must explicitly authorize and securely deliver this grant to the intended device; the desktop never receives owner credentials. Recovery uses the existing enrolled metadata, not caller-supplied identity fields. Grant issuance leaves the old token valid. Exchanging the grant through `/v1/enroll` returns 201 with a new device token and atomically invalidates the old token, retaining UUID, name, platform, architecture, current snapshot, and revision. A lost enrollment response can be recovered by requesting another owner-issued recovery grant for the same UUID.

Recovery grants pin the current credential hash when issued. Multiple grants issued for the same credential generation may coexist; once one rotates the credential, another cannot supersede it. Stale recovery grants return 409 and are consumed atomically; another use returns 401. Revocation between issuance and enrollment also rejects and consumes the grant, and revoked IDs cannot obtain recovery grants. Expired grants return 401 and do not rotate credentials. The original `/v1/pairing-grants` still returns 409 for any already enrolled ID. Existing persisted schema-1 grants without the new rotation fields default to initial enrollment; no migration or credential reset occurs.

Devices share one owner's roster and snapshots. This access is explicit: any enrolled device can read inventories of every other active device under this owner. No tenant/account distinction exists. Owner credentials cannot write snapshots; device credentials cannot issue grants or revoke devices. Revocation permanently tombstones the ID, so later pairing cannot restore the old credential/identity. Revoked snapshots are logically deleted; backups or database free pages may retain old data. Storage is not encrypted or end-to-end encrypted, and the service owner can read inventories.

First snapshot uses revision 1. Every subsequent write must use exactly the stored revision plus one; replayed, stale, zero, and skipped revisions return 409 without changing state. Revision and snapshot commit atomically. No retry silently overwrites a newer snapshot. Roster supplies stored revisions. Scan timestamps are reported facts and are not used for ordering.

```json
{
  "revision": 1,
  "snapshot": {
    "device": {
      "id": "9c1f7c60-e081-4e69-b073-7f82d2c925fb",
      "name": "Workstation",
      "platform": "windows",
      "architecture": "x86_64"
    },
    "scanned_at_unix_secs": 1791676800,
    "apps": [],
    "warnings": []
  }
}
```

Domain errors return `{"error": "fixed message"}`; JSON extraction errors use axum's standard rejection body. Status meanings: 400 validation; 401 missing/bad/expired/revoked credentials; 403 wrong credential role or cross-device write; 404 absent snapshot/device; 409 revision/identity/capacity conflict; 413 oversized JSON body; 422 malformed JSON schema; 503 concurrent request capacity; 500 storage/entropy failure. A conflicting initial enrollment does not consume its grant, but cannot overwrite an existing identity. Stale or revoked credential-recovery grants are consumed on their 409 response. No plaintext token is stored or logged by this service.

JSON bodies are limited to 2 MiB; shared validation also caps 10,000 apps and 64 warnings, checks text sizes/control characters and duplicate app IDs. At most 64 device identities (including revocation tombstones), 64 pending grants, 16 concurrent requests, and 16 running/queued blocking database operations are allowed. Blocking-work permits remain owned by database tasks even if their HTTP caller disconnects. Expired grants are pruned when owner requests another grant. Active snapshots have one retained logical version per device. There is no production network rate limiter, connection/read deadline, audit trail, hardware-backed secret storage, owner rotation, or tenant isolation. Grants require a reliable system clock; enrollment rejects timestamps before grant issuance as well as expired grants.

## Future desktop integration

1. Obtain explicit owner consent and configured service transport. Keep local inventory local by default.
2. Pair using the desktop's persisted canonical UUID and shown device metadata. Present the single-owner read-sharing policy before enrollment.
3. Exchange the grant exactly once. Put the returned device token in an OS credential store; never fall back to plaintext disk or ship owner credentials. If the response is lost or secure storage fails after enrollment, retain the local UUID and request an owner-issued recovery grant through `/v1/devices/{id}/pairing-grants`; exchange that grant to rotate credentials and retry secure storage. A revoked ID cannot recover.
4. Read roster revision, then submit a validated local snapshot with the next revision. On ambiguous timeout, read current server state before choosing a revision; do not blindly replay writes.
5. Treat 401 as revoked/expired enrollment requiring user action. Apply bounded retries/backoff to 503/offline failures, and refresh revision on 409. Never interpret synced inventory as executable commands.

No client integration is included here. Cloud account login should use a separately selected identity provider and a reviewed native OAuth/OIDC flow, with account isolation designed and tested before multiuser deployment.

## Validation and API references

```powershell
cargo test --manifest-path sync-server/Cargo.toml
cargo clippy --manifest-path sync-server/Cargo.toml --all-targets -- -D warnings
```

Tests cover unauthenticated/bad tokens, owner/device authorization, concurrent grant reuse and expiry, revocation, cross-device writes, metadata and canonical ID validation, oversized bodies, stale/skipped revisions, pending/concurrent request bounds, canceled-caller blocking-work bounds, restart persistence, owner pinning, unrelated/schema-less file preservation, abandoned initialization retry, and exclusive publication preservation, owner-only recovery, retained identity/snapshot/revision, old-token invalidation, recovery races/revocation, and backward-compatible persisted grants. This is functional validation, not a completed security audit.

Implementation uses current primary references: [axum router](https://docs.rs/axum/0.8.9/axum/routing/struct.Router.html), [body limits](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html), [redb transactions](https://docs.rs/redb/4.3.0/redb/), [OS randomness](https://docs.rs/rand_core/0.9.5/rand_core/struct.OsRng.html), [SHA-256](https://docs.rs/sha2/0.10.9/sha2/), [constant-time equality](https://docs.rs/subtle/2.6.1/subtle/trait.ConstantTimeEq.html).
