# Development deployment

Approved and published on 2026-10-13: Kitlane development account service and browser sign-in bridge. Existing projects and the Kitlane production branch remain untouched.

- Neon project: `floral-darkness-58175880` (Kitlane, AWS Frankfurt).
- Development branch: `br-damp-cell-batye88a`.
- Function slug: `kitlane`, runtime Node 24.
- HTTPS origin: `https://br-damp-cell-batye88a-kitlane.compute.c-8.eu-central-1.aws.neon.tech`, registered as a Neon Auth trusted domain.
- Active deployment: 10; runtime-injected database/Auth configuration and exact origin/issuer/audience configured.
- Artifact: `services/account-api/dist/kitlane-api.zip` (201,571 bytes), containing the bundled `index.mjs` API and official SDK browser bridge.
- SHA-256: `3803D56191983F42281E305A64DD535B50CA483B0C8CF92E57C6554CB22DA97F`.
- The approved initial artifact was `86300DC69411E9B4BE6C278E502ED4785230D198540CA30D332913424248750B`; live testing exposed Google's root return, fixed in deployment 3 with a regression test.
- Source/config: `services/account-api/src`, `web`, `neon.ts`, `.env.example`; no secrets in the bundle or Git.

## Deployment and verification

1. Completed: deployed with injected database/Auth configuration; configured `KITLANE_ORIGIN` and Neon Auth trusted domain to the native HTTPS origin. The temporary provisioning placeholder was replaced before testing.
2. Completed: actual Google-session JWT issuer and audience are both the Auth origin. Updated the exact issuer, retaining all claim/signature checks. Temporary diagnostics logged public configuration only and were removed.
3. Completed: hosted health/assets/security headers and unauthenticated rejection, real Google/native callback, vault restore/refresh/sign-out. Same/foreign-account pairing and concurrent consumption passed on the isolated test database. Physical two-computer and real foreign-account browser checks remain manual. Shared Google OAuth credentials are enabled for development; production requires the owner's Google OAuth app and email/password disabled in Neon Console.
4. Completed: native executable/installer built with this verified development API origin; isolated install/uninstall passed.

## Validation completed

- Twelve server tests passed, including actual isolated Neon transactions: cross-account/unpaired access, single-use races, UUID-adoption rejection, expiry/PKCE, session revocation, caller quota isolation, Google origin-root return, SDK session-cache JWT regression, and invalid-session rejection. Typecheck and Function/browser bundle build passed.
- Native workspace: 135 tests passed, two optional tests ignored; formatting and Clippy clean. Mandatory login, fresh consent, retained local inventory, vault failure, expired sign-out, callback cancellation/deadline, metadata update, and window controls covered.
- Native light/dark and narrow code-wizard screenshots inspected. Asset validation passed 398 checks. Windows icon diagnostic found 110 usable icons from 144 inventory entries; fallback tiles remain for entries without usable resources.
- Independent review completed; five important findings fixed. The onboarding name issue was also fixed because it affects the normal first-launch flow. Release compilation succeeded. Installer validation rejects missing/non-HTTPS API origins.
- Hosted health/config/bridge requests passed, required security headers present, unauthenticated account requests return 401. Root return serves the bridge. Real Google authorization, PKCE callback/exchange, OS-vault persistence, new-client restoration, refresh rotation, authenticated request, remote sign-out, and local token removal passed. No inventory was uploaded.
- Configured Windows installer compiled. Isolated install/uninstall used a fresh AppId and workspace directory with no shortcuts or app launch: installed binary matched, uninstall removed it, retained local-data sentinel was unchanged.
- Real Windows credential-store round trip passed with one temporary test entry, removed afterward. Formatting and Clippy passed including the opt-in `account-client/examples/live-check.rs` diagnostic.

## Live checks remaining

Two physical computers and a real foreign-account browser flow remain unverified. Their authorization and race behavior passed actual isolated Neon transactions. Application quotas are credential/installation-scoped; production ingress protection must be configured separately. The deployed service and installer target development; production remains untouched.
