# Neon Google accounts and device pairing implementation plan

> **For agentic workers:** Use superpowers:executing-plans for native execution, or superpowers:subagent-driven-development if the user chooses that method. Keep task ownership explicit and preserve other work.

**Goal:** Require verified Google login before entering Kitlane and replace local owner tokens with expiring pairing codes between devices on the same account.

**Architecture:** A separate Neon project hosts Managed Auth, Postgres, and a small account Function. A browser bridge uses the official Auth SDK; a PKCE-protected loopback handoff creates native device sessions. The Rust worker owns network operations and OS-vault access. Local inventory and preferences remain independent of account state.

**Tech Stack:** Native Rust/egui, existing blocking HTTP client and credential vault, TypeScript/Hono, PostgreSQL, Neon Managed Auth, Neon Functions, official Neon Auth browser SDK, JOSE verification.

**Spec:** [Approved account design](../../../app/docs/neon-account-design.md).

## Global constraints

- Recheck current official Neon SDK/API contracts before implementation. Record SDK versions and actual token claims; do not guess issuer, audience, or Google provider evidence.
- Provision only a new Kitlane project in AWS Frankfurt and a development branch. Do not change unrelated projects. Test development before requesting production deployment approval.
- Keep database/provider secrets server-side; native credentials only in the OS vault. Ignore local env files before creating them. Redact transport errors and never log request bodies, authorization headers, callbacks, or credentials.
- Release workspace and onboarding require a validated account session. No local-service or preview bypass in the release executable. Preview fixtures remain isolated from storage/network.
- Login and pairing do not upload inventory. Sync requires current account consent and explicit action. Local icon locations/bytes never enter snapshots.
- Preserve native accessibility, both themes, reduced motion, and local inventory/settings. Meaningful completed tasks get separate commits.

## Review focus

1. A valid Neon JWT from a non-Google account must be refused: server tests for authoritative Google provider evidence and missing/ambiguous evidence.
2. Two concurrent consumers must never reuse a handoff, refresh credential, or pairing code: transactional race tests on a development database.
3. UUID substitution must not expose another account's devices or snapshots: composite foreign keys plus cross-account and unpaired read/write tests.
4. Session expiry, cancellation, vault failure, and sign-out must leave the native gate closed: worker/UI tests with injected clients/vaults, including account switches.
5. Loopback requests can be unsolicited: bounded callback parsing, state/PKCE checks, timeout, replay, wrong path/host, and secret-redaction tests.

## Task 1: Server contract, schema, and development project

**Create:** `services/account-api/package.json`, `tsconfig.json`, `neon.ts`, `src/config.ts`, `src/contracts.ts`, `migrations/001_accounts.sql`, `test/schema.test.ts`, `.env.example`, `README.md`.

**Modify:** root `.gitignore`; add workspace membership only if existing package workspace requires it.

**Interfaces:** Account responses contain `account_id`, display email/name, `device_id`, and expiry. Pair codes contain `code` and `expires_at`. Native inventory retains the existing `InventorySnapshot` shape. All native API routes use `/v2` and derive account/device authorization from credentials.

- [ ] Verify official Auth/Functions docs and installed SDK interfaces; document JWT claims and provider-check strategy. Fail closed if provider evidence cannot be verified.
- [ ] Add ignored env patterns; scaffold locked TypeScript dependencies and test/typecheck scripts.
- [ ] Write schema tests rejecting cross-account foreign keys and duplicate/self pair edges. Run tests to establish the missing schema failure.
- [ ] Create separate Kitlane Neon project/dev branch, provision Better Auth, and inspect redacted provider configuration. Record public IDs/URLs only.
- [ ] Implement `kitlane` tables for accounts, devices, hashed sessions, handoffs, pairing codes, ordered pair edges, snapshots, and bounded rate-limit buckets. Account ID derives from the verified Neon subject. Add TTL/revocation fields and indexes for cleanup/authorization.
- [ ] Apply schema on development only; run constraints tests and typecheck. Commit schema/config/docs without credentials.

## Task 2: Google browser bridge and native session handoff

**Create:** `services/account-api/src/auth.ts`, `src/handoff.ts`, `src/sessions.ts`, `src/index.ts`, `web/sign-in.ts`, `web/sign-in.html`, `test/auth.test.ts`, `test/handoff.test.ts`.

**Interfaces:** `POST /v2/auth/attempts` accepts bounded loopback callback, random state, SHA-256 PKCE challenge, device UUID/name/platform; returns browser URL and attempt expiry. Authenticated browser completion verifies Neon JWT and Google provider, creates a one-use code, then redirects only code/state to the stored callback. `POST /v2/auth/exchange` accepts code/verifier/device ID, returns opaque access/refresh credentials plus account information. `POST /v2/auth/refresh`, `GET /v2/account`, `POST /v2/auth/logout` rotate, validate, and revoke sessions.

- [ ] Write tests for JWT signature/issuer/audience/expiry, unknown keys, non-Google identities, malicious callback targets, state/PKCE mismatch, code replay/expiry, refresh replay, and sanitized errors. Confirm failures before implementation.
- [ ] Build bridge with official Neon social-login SDK and strict origin/CSP/referrer policy. Bundle SDK locally; no credentials in HTML or URLs. Browser errors/cancellation show retry guidance.
- [ ] Implement bounded handoff creation/completion/exchange with atomic one-use consumption. Verify identity using trusted JWKS and authoritative provider evidence.
- [ ] Implement hashed opaque sessions with short access TTL, rotating refresh TTL, revocation, explicit transactions, and account/device binding. Do not issue usable credentials before the exchange succeeds.
- [ ] Run auth/handoff tests, typecheck, and bundle. Commit server and bridge.

## Task 3: Account-scoped pairing and inventory API

**Create:** `services/account-api/src/pairing.ts`, `src/inventory.ts`, `src/limits.ts`, `test/pairing.test.ts`, `test/inventory.test.ts`.

**Interfaces:** `POST /v2/pairing/codes` generates eight unambiguous characters, TTL five minutes. `POST /v2/pairing/consume` accepts a code and creates an undirected connection. `GET /v2/devices` lists paired peers only. `PUT /v2/inventory` accepts this session's validated snapshot. `GET /v2/inventory` returns snapshots of paired peers only; `DELETE /v2/devices/:id` removes a pair without deleting local inventory.

- [ ] Write tests for expiry, invalid/foreign codes with identical errors, self-pairing, guess limits, concurrent consumption, device substitution, unpaired snapshot access, payload limits, and no writes on login/pairing. Confirm expected failures.
- [ ] Implement transactionally consumed codes, server-derived IDs, composite authorization queries, and bounded rate limits. Use a cryptographic code generator and hash stored codes.
- [ ] Validate inventories against existing Rust contracts: bounded size/counts, known platform/architecture enums, UUIDs, timestamps, optional fields; reject icon/path extras. Keep explicit consent as the native upload gate.
- [ ] Run server tests against an isolated dev test namespace, typecheck, and bundle. Commit API.

## Task 4: Rust account client, vault, and system-browser handoff

**Create:** `app/account-client/Cargo.toml`, `src/lib.rs`, `src/handoff.rs`, `tests/account.rs`.

**Modify:** `app/Cargo.toml`, `Cargo.lock`, `app/sync-client/src/lib.rs` only to expose/reuse the existing OS vault with fixed safe errors if necessary.

**Interfaces:** `AccountClient::restore`, `begin_sign_in`, `complete_sign_in`, `sign_out`, `create_pairing_code`, `pair`, `devices`, `sync`. Return typed account/session summaries without secret-bearing Debug output. Sign-in cancellation uses an atomic cancellation signal; callback listener binds only `127.0.0.1:0`.

- [ ] Write injected-vault/local-test-server tests for endpoint HTTPS enforcement, redirects/timeouts/body limits, state/PKCE mismatch, unsolicited callback, cancellation, replay, refresh failure, vault write failure, sign-out cleanup, and secret redaction.
- [ ] Implement HTTPS client with no cross-origin credential redirects or implicit unsafe retries. Reuse existing transport/vault dependencies. Save rotating credentials only after validation; fail closed if vault persistence fails.
- [ ] Implement bounded loopback parser and random state/PKCE with OS entropy. Open the browser through the native integration; close listener on success/cancellation/timeout. Never expose bearer credentials in browser URLs.
- [ ] Run crate tests and workspace fmt/Clippy. Commit client.

## Task 5: Native Google gate and account-aware worker

**Create:** `app/src/account.rs`.

**Modify:** `app/src/worker.rs`, `shell.rs`, `view.rs`, `bin/ui-preview.rs`, `theme.rs` only for required sign-in assets/helpers.

**Interfaces:** Worker commands/events carry `RestoreAccount`, `SignIn`, `CancelSignIn`, `SignOut`, `AccountReady`, `SignedOut`, safe account errors. UI states are restoring, signed out, signing in, signed in. Only signed-in state can render onboarding/workspace or issue scan/sync/pair commands.

- [ ] Write UI/worker tests for mandatory gate, startup restoration, revoked session, cancel/retry, sign-out, account switch clearing cloud peers/consent, preserved local inventory/preferences, and no login upload. Add synthetic preview states for every gate state.
- [ ] Implement responsive branded Google sign-in screen and account controls. Keep network/vault operations off the UI thread; show pending/cancel/retry states and useful configuration errors.
- [ ] Route release worker exclusively through the account client. Keep legacy local prototype available only as a separate development service/client.
- [ ] Verify both themes, keyboard focus, reduced motion, narrow window screenshots, workspace tests and Clippy. Commit native gate.

## Task 6: Replace pairing wizard with account codes

**Modify:** `app/src/pairing.rs`, `shell.rs`, `worker.rs`, `view.rs`, `bin/ui-preview.rs`, affected UI tests.

**Interfaces:** Wizard offers Create code / Enter code, shows current account, expiry/copy state, then connection confirmation. Commands/events include GenerateCode, PairCode, PairedDevices. Devices renders only authorized paired peers and centers the connection CTA when empty.

- [ ] Write tests for eight-character entry, expiry/regeneration, failures/retry, copy feedback, same-account instructions, disconnected peers, and no automatic scan/upload after pairing.
- [ ] Replace service-address and 64-character token fields with padded code controls. Keep consent tied to the current account and explicit Sync action. Clear transient codes when closing or signing out.
- [ ] Preserve clean inputs/radios, vertically centered Lucide titlebar icons, subtle scrollbars, and local program icons.
- [ ] Run both-theme/narrow/reduced screenshots and relevant UI/client tests. Commit wizard.

## Task 7: Configuration, live verification, and release packaging

**Modify:** `services/account-api/README.md`, `app/README.md`, `app/docs/neon-account-design.md`, `app/installer/build.ps1` only as needed for API configuration; add public configuration sample.

- [ ] Document Google Console OAuth client/callback, Neon trusted origin, SDK/provider setup, deployment command, API build URL, database migrations, session revocation, and recovery. No secrets in chat/Git.
- [ ] Build dev Function artifact and bridge, validate local server/client integration and transactional races. Present concrete dev deployment/configuration for approval before publishing.
- [ ] After authorization, deploy development API/bridge; test real Google login and two same-account devices plus a foreign account. If credentials or a second device are unavailable, record precise unverified scenarios; never simulate successful login in release.
- [ ] Run final server tests/typecheck/build, Rust workspace tests/fmt/Clippy, asset validation, release build, installer compilation, and safe installer smoke checks where needed. Inspect Git diff for secrets.
- [ ] Package required license notices, commit release configuration/docs, and report verified behavior and remaining live configuration limits.

## Self-review

All approved spec requirements map to Tasks 1–7. Endpoint names and native command/event ownership are consistent across server/client/UI tasks. Tests cover the five review focus areas. Production hosting/provider setup and two-device validation remain explicit external prerequisites rather than implicit success claims.
