# Native account client

Kitlane's release worker uses this HTTPS client exclusively. The legacy loopback client remains a separate development prototype; its owner tokens cannot unlock the GUI.

Build the desktop with `KITLANE_ACCOUNT_API` set to the account API's exact HTTPS origin. An unset or invalid address keeps the login gate closed and shows a configuration message. Credentials use the existing native keyring implementation under a separate `cloud-v2` vault key derived from origin and device UUID. There is no plaintext credential fallback.

Sign-in creates random state and a PKCE verifier, binds `127.0.0.1:0`, opens the system browser, and waits at most five minutes. Only the bounded `/kitlane/callback` request with the expected Host, state, and one code is accepted. The verifier goes only to the HTTPS exchange endpoint. Cancel/close stops the listener; pending HTTP calls have a fifteen-second timeout. Redirects cannot forward credentials.

Startup rotates and validates the saved refresh credential before opening onboarding or the workspace. The worker checks session expiry while idle and before work; unusable credentials close the gate and clear cloud consent/state. Vault failures and revoked sessions keep the gate closed. Sign-out revokes through the refresh credential, so an expired access credential cannot prevent revocation, and removes local account credentials. Network/vault failures use fixed messages without secret-bearing transport errors.

An installation proof is generated lazily in the OS vault at first sign-in and retained across account sign-outs. Its hash binds server registration to the physical installation and prevents another client from adopting its UUID/pair edges. It grants no account session by itself. Loss of this proof requires registration recovery; there is no insecure ownership bypass. Onboarding updates device metadata through an authenticated endpoint without uploading inventory. Callback header reads have a two-second absolute deadline and check cancellation every bounded read.

Run `cargo test -p kitlane-account-client`. Transport tests need permission for loopback sockets. They use memory vaults and synthetic credentials; no real Google account or OS vault entries are written.
