# AI suggestions and automatic updates

Requested scope: AI alternatives, updates for selected installed programs, and Kitlane self-updates. Version 0.2.0 adds these as separate local features; defaults remain disabled until configured in Settings.

- AI: explicit per-app requests send only the selected app, workflow preferences and relevant candidate facts to TypeSafe Jev. Keys use the OS vault or existing environment configuration. Typed model judgments rank source-backed alternatives; code validates candidates and constraints. Accepting a recommendation records a choice, not an installer command.
- Program updates: per-program opt-in using exact WinGet identities. Microsoft source and protected provider executable are checked. Respect pins and known versions. Re-inventory before mutation and after completion; record outcomes. Never upgrade all programs, accept new agreements silently, override hashes/pins, reboot, or interrupt a live installer.
- Background: a separate per-user agent continues after the UI closes and starts at login when enabled. A bounded durable queue and job history live separately from the UI database. Interrupted work requires reconciliation before another attempt.
- Kitlane: signed, expiring update metadata binds version, sequence, platform, artifact hash and size. A public trust root ships with the app; private release signing material stays in the credential vault or CI secret. Windows updates use the verified installer after the UI closes. Public release publication is separate from implementation and local verification.

Validation covers malformed inputs, source/provider identity, model abstention, preferences, secure-key failures, durable policy and crash recovery, signing/expiry/replay/downgrade checks, and UI behavior. Live AI evaluation uses a small representative sample; real third-party updates are not run on the development machine. A local installer is prepared for inspection before signing and public release.

## Validation on 2026-10-15

213 workspace tests passed, with five opt-in tests ignored in the default run. Strict workspace Clippy and formatting passed. Core all-target Clippy compiled for Linux x86-64 and macOS ARM64. Four live TypeSafe cases passed: browser preferences, CAD routing, office routing and incompatible-workflow abstention. The explicitly run read-only Windows provider test resolved Microsoft's protected WinGet executable and validated its official source; its no-pins command was also checked without upgrades.

The Windows release installer built with both executables. Installed 0.2.0 executable hashes matched the release build, and the existing redb database was byte-for-byte unchanged. Default policies remained disabled, startup registration absent, and the idle agent exited. Native fixture screenshots covered Settings and Library controls. Vendor installer/elevation/restart integration still needs disposable machines; no real third-party upgrade was executed.

After explicit maintainer approval, release key `kitlane-release-2026-01` was created in the OS credential vault and its public key pinned in `update-root.json`. On 2026-10-15, the matching seed was transferred directly through authenticated GitHub CLI stdin to the encrypted repository Actions secret `KITLANE_UPDATE_SIGNING_KEY`; no plaintext key file, command argument or log was used. This trust root enables Kitlane's self-update setting in rebuilt binaries; automatic policies remain opt-in. Public release publication is gated on all five native packages passing CI.

The rebuilt local Windows installer was signed with sequence 1. Independent Node Ed25519 verification checked the signature, artifact hash/size, expiry and tamper rejection using only the pinned public key. All 213 workspace tests passed again. The rebuilt main and agent were installed and their hashes verified; both the database and maintenance preferences remained byte-for-byte unchanged.
