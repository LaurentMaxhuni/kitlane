# Account implementation rulings

Approved plan: `docs/superpowers/plans/2026-10-13-neon-accounts.md`. Implemented on `codex/neon-accounts`; development deployment approved separately. Production remains untouched.

1. Reuse the existing checkout on a feature branch to preserve approved UI changes and build artifacts after the user chose this session. Cost if wrong: files are shared with concurrent local editors.
2. Require a configured JWT audience until observed from a real Google session, avoiding guessed or omitted claim validation. Cost if wrong: setup cannot proceed without the audience. The live Google audience was verified.
3. Configure the issuer rather than hardcode an anonymous-token assumption. Cost if wrong: a real Google token requires adjustment before release. Live Google verification confirmed the Auth origin; the configuration was corrected without weakening validation.
4. Retain account-independent installation proof in the OS vault across sign-out while removing account tokens, preventing device UUID adoption. Cost if wrong: losing the vault requires identity recovery.
5. Bind quotas to private installation/handoff/session proofs because Neon documents no trustworthy caller-IP header. Cost if wrong: production still needs separate ingress protection against distributed resource abuse.
6. Read the official SDK's authenticated session JWT instead of `.token()` after `getSession()`: SDK 0.6.0-beta shares those endpoint caches and returns a session-shaped response. Cost if wrong: an SDK update must preserve or revise extraction; opaque tokens fail closed. The actual SDK regression and live Google/native flow passed.
7. Serve the bridge at both `/sign-in` and the origin root, retaining only the pending attempt identifier in the same tab for five minutes because Managed Google OAuth can return to the root. Cost if wrong: browsers blocking session storage need a new sign-in if the callback loses its path. Google, account policy, PKCE, and single-use handoff checks still authorize the device.

Deferred minor review findings: none. The onboarding name finding was regraded Important and fixed.

Final verification: 12 server tests including isolated Neon transactions; 135 native tests passed, two optional tests ignored; separate Windows vault round trip passed; formatting/Clippy and 398 asset checks passed; configured release/installer and isolated install/uninstall passed. Real Google authorization, native PKCE callback/exchange, vault persistence/restoration, refresh rotation, authenticated request and sign-out passed without inventory upload.

Unverified: two physical computers, a real foreign-account browser flow, non-Windows runtime and manual screen-reader use. Account isolation/pairing races passed isolated database tests. Shared Google credentials and the deployed endpoint are development configuration; production provider and ingress setup remain separate.
