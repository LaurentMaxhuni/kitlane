# Kitlane blocking desktop sync client

Explicit inventory upload for the Rust loopback prototype. `Client::new(endpoint, &persisted_device)` validates the endpoint and device before networking. Use `DEFAULT_ENDPOINT` (`http://127.0.0.1:8787`) unless the owner configured another local port. The endpoint must be exactly `http://127.0.0.1:<port>`; URLs with paths, credentials, query parameters, fragments, alternate hosts or HTTPS are rejected. Redirects, environment/system proxies, cookies, compression and automatic transport retries are disabled. This adapter has no remote transport.

Run every method on the desktop's blocking worker. `is_enrolled()` checks local OS credential presence only. `enroll(pairing_token)` exchanges a 256-bit owner-issued single-use grant once, validates the returned device against every persisted metadata field, then stores the device credential in the OS vault. It refuses to replace an existing credential. Failed or unconfirmed enrollment needs owner assistance and an explicit recovery grant. Returned tokens and raw server/vault errors never appear in public errors or debug output.

`sync(&local_snapshot)` requires an explicit user action. It validates local inventory, reads the roster and current snapshots, validates each enrolled device and exact revision, then writes the next revision only if local inventory differs. If an earlier ambiguous PUT already committed the same inventory, the next explicit sync reads it back and skips the write. Revision races return `Conflict` without retries. The returned `SyncResult.peers` excludes this device and devices without uploaded inventories. Any malformed peer causes the operation to fail before upload.

Each HTTP request has a 10-second deadline and 2-second connect deadline; each sync has a 30-second operation deadline. Response JSON, total inbound JSON per sync, and each upload are limited to 2 MiB. Rosters contain at most 64 unique validated identities. Shared inventory validation caps records and text fields, including optional native identifiers and package origins. Large fleets can exceed the aggregate limit and receive `TooLarge` before uploading.

`disconnect()` deletes this endpoint/device's local vault entry only; the owner's server enrollment remains active. HTTP 401 requires user action for expiry/revocation. Endpoint and device UUID determine a SHA-256 credential account namespace under `kitlane.sync.loopback.v1`, preventing accidental reuse across configured services or identities. A replacement server on the same port is the same prototype endpoint: this transport has no cryptographic server authentication.

Production vaults use Windows Credential Manager, macOS Keychain, and Linux keyutils backed by Secret Service. Linux builds require DBus development libraries (`libdbus-1-dev` on Debian/Ubuntu); runtime requires an unlocked Secret Service and usable session keyring. Unsupported platforms or unavailable stores fail closed. There is no plaintext credential fallback. OS permission/unlock dialogs can outlast HTTP deadlines. Windows compilation and protocol tests do not prove macOS/Linux keychain access; native vault smoke tests require those platforms and consenting users.

Protocol tests inject `MemoryVault` via `Client::with_vault`, without reading or writing the user's credential store. Tests cover the real Rust service and scripted local HTTP failures, enrollment uncertainty, vault failure, namespace isolation, authorization, redirects, revision conflicts, ambiguous write recovery, peer validation, size bounds and old/new inventory wire compatibility.

An opt-in native vault test creates one unique test entry and removes it afterward (including cleanup on unwinding). It passed on Windows; macOS/Linux vault runtime checks remain pending:

```powershell
cargo test -p kitlane-sync-client native_credential_store_round_trip -- --ignored
```

```powershell
cargo test --manifest-path app/sync-client/Cargo.toml
cargo clippy --manifest-path app/sync-client/Cargo.toml --all-targets -- -D warnings
```

API references: [reqwest blocking client](https://docs.rs/reqwest/0.13.5/reqwest/blocking/struct.ClientBuilder.html), [keyring 3.6 platform stores](https://docs.rs/keyring/3.6.3/keyring/), [keyring features](https://docs.rs/crate/keyring/3.6.3/features). Version 3.6 is used for its explicit native-store feature selection; the 4.x umbrella crate recommends applications with explicit store selection use its separate core/store ecosystem.
