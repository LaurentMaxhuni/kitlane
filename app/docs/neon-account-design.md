# Google accounts and device pairing — approved design

Status: approved by the user on 2026-10-13; server, bridge, native gate, and pairing code flow implemented and tested. Development HTTPS deployment and real Google authorization, native PKCE callback, OS-vault persistence/restoration, refresh rotation, and sign-out passed. Pairing and account isolation passed on an isolated Neon database; two physical computers and a real foreign-account browser flow remain unverified. This replaces the desktop's single-owner local-service workflow. The native Rust UI stays native; browser sign-in uses Neon's supported Auth SDK.

## Intended experience

1. The installer launches Kitlane into a branded Continue with Google screen. Library, Devices, and onboarding require a verified session. A valid saved session restores login; expired or revoked sessions return to sign-in.
2. Continue with Google opens the system browser. Google authentication uses Neon Managed Auth. Successful browser authentication returns control to the same desktop sign-in attempt.
3. Both computers sign in to the same Google account. One generates a pairing code; the other enters it in the padded connection wizard. Users no longer enter service addresses or owner-issued 64-character tokens.
4. Devices lists only explicitly paired computers. Scan and Sync remain user-controlled. Signing in or pairing does not upload inventory.
5. Settings shows the current account and Sign out. Signing out clears credential material and cloud device state, while retaining local inventory and appearance preferences. Switching accounts requires fresh sharing consent.

## Neon components

Use a separate Kitlane project on AWS Frankfurt (`aws-eu-central-1`), with a development branch for schema and auth changes. Existing unrelated Neon projects remain untouched.

- **Managed Auth:** Google OAuth through the official Neon SDK. Register the browser bridge's trusted HTTPS origin and the Google provider's callback URL. Configure Google credentials in Neon; none ship in the desktop executable. The service must enforce the Google-only identity policy, rather than merely hiding other login buttons.
- **Account API on Neon Functions:** a small TypeScript service validates authentication and owns pairing, sessions, device registration, and snapshot access. Postgres is accessed by server handlers. No database credentials or direct Data API access from the desktop.
- **Browser sign-in bridge:** hosted beside the API, using the supported SDK's social sign-in and verifier handling. No Google password collection or embedded browser.
- **Rust desktop client:** HTTPS requests to a configured account API, with credentials in the existing OS credential-store abstraction.

## Native sign-in handoff

The desktop creates a random state and PKCE verifier and binds an ephemeral loopback listener. The server accepts only loopback callback destinations and a bounded port/path format. The bridge authenticates with Neon, then redirects a short-lived one-use authorization code and state to the listener. The desktop checks state and exchanges the code plus verifier over HTTPS.

The server verifies Neon token signature, issuer, audience, expiry, and stable subject using trusted Auth metadata/JWKS. It must also enforce the Google-only policy. Account identity derives from the verified subject, never from an email address supplied by a client. Browser cookies and the SDK's Neon verifier stay with the supported browser implementation.

The exchange issues a device-bound session: a short-lived access credential and rotating refresh credential. Store only hashes of opaque credentials in Postgres and the credentials themselves in the OS vault. Reject expired, reused, mismatched, or revoked handoffs. Cancel attempts after five minutes. Do not put bearer credentials in redirect URLs or logs. Startup verifies/restores the session before exposing the workspace; sign-out has no local-library bypass.

Independent review added installation ownership proof: a random installation secret stays in the OS vault across account sign-outs, while Postgres stores its hash. Reusing an existing account/device UUID requires the same proof, so knowing another computer's UUID cannot inherit its connections. This proof alone grants no account access; Google authentication remains mandatory. Losing the OS vault requires identity recovery before an existing registration can be reused.

## Pairing and isolation

Generate an eight-character code from an unambiguous alphabet, valid for five minutes and consumed atomically once. Pairing requires authenticated devices belonging to the same account and is rate-limited; use one error response for invalid, expired, or foreign-account codes. A successful code creates an explicit undirected connection between two devices.

Proposed tables in a separate `kitlane` schema: device registrations, device sessions, sign-in handoffs, pairing codes, pair connections, and inventory snapshots. Device and connection keys include the account ID, with composite foreign keys preventing cross-account references. Every read and write derives account and device context from validated credentials. Snapshot reads require both account membership and an existing pair connection; body-supplied IDs cannot grant access.

Keep the existing `/v1` local-service prototype as a development tool. The release GUI uses the account API exclusively, so legacy owner tokens cannot bypass required Google login. Do not migrate old local-service credentials into cloud accounts. Preserve local device identity and inventory.

## Checks before enabling the release gate

- Real Google login, browser return, session restoration, cancellation, expiry, sign-out, and revoked credentials.
- Missing/invalid JWT signatures and claims, unknown signing keys, bad callback origins, state/PKCE mismatches, replay, and token redaction.
- Concurrent code consumption, expiry, repeated guesses, self-pairing, cross-account pairing, device-ID substitution, and unpaired snapshot access.
- No uploads on login or pairing; local icon paths and cached icon data never enter snapshots.
- Native light/dark sign-in and pairing screens, keyboard operation, narrow windows, and reduced motion.

Live Google verification requires the project's Google OAuth configuration and a deployed HTTPS bridge/API. Build and test the code and development branch before asking for deployment approval. Record configuration instructions instead of accepting secrets in chat.

## Primary references

- https://neon.com/docs/auth/authentication-flow
- https://neon.com/docs/auth/guides/manage-auth-api
- https://neon.com/docs/compute/functions/authentication
- https://developers.google.com/identity/protocols/oauth2/native-app

Implementation must recheck the current SDK and endpoint contracts before coding. This design does not promise that enabling Neon Auth alone supplies native login or account authorization.
