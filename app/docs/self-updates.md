# Signed Kitlane updates

Kitlane's preview channel automatically checks the official
`LaurentMaxhuni/kitlane` GitHub releases. Windows x64 installations in
`%LOCALAPPDATA%\Programs\Kitlane` can stage a signed update and run the Inno
installer after the UI closes. The maintenance agent exits after launching the
installer so its own executable can be replaced. Other platforms report that
automatic installation is unsupported and use the official release download.
Portable/custom-location Windows copies also require a manual installation.

## Trust and wire format

`app/update-root.json` is compiled into each binary. Its Ed25519 public keys,
revoked key IDs, and minimum accepted sequence are the trust anchor. Remote
metadata cannot add keys or remove revocations. The release asset
`update-preview-v1.json` contains `key_id`, `payload` (base64 exact UTF-8 JSON
bytes), and `signature` (base64 64-byte Ed25519 signature). The signature covers
the bytes `kitlane-update-v1\0` followed by the decoded payload. This avoids
cross-protocol signature reuse and JSON reserialization ambiguity.

The signed payload binds schema, repository, channel, monotonic sequence,
canonical release version, publication and expiry Unix timestamps, OS,
architecture, installer size, and lowercase SHA-256. Verification uses Dalek's
`verify_strict`. Metadata is valid for at most 31 days; publishing tools issue
30-day metadata. The version must exceed the installed binary's version, and
the target must exactly match Windows/x86_64 and the compiled preview channel.
Installer filename and URL are derived locally from the validated version.
Neither unsigned `SHA256SUMS.txt` nor arbitrary server commands/paths authorize
an installation.

The updater bounds release-index responses to 1 MiB, signed envelopes to 64 KiB,
and installers to 512 MiB (also enforcing their exact signed size). HTTPS
requests start at fixed GitHub API/release paths. Asset redirects are limited to
three hops and the exact `github.com` official repository download path or
`release-assets.githubusercontent.com` origin. TLS certificate verification
remains enabled. Hash and signature are checked again immediately before
launching the installer. Installer arguments and destination are fixed locally.

`accepted-update.json` stores the highest accepted checked sequence and payload
digest in Kitlane's state directory. Lower sequences and differing payloads
reusing the same sequence are rejected; an identical manifest can be retried.
Corrupt history fails closed. This protects against replay within retained
per-user state, not a privileged attacker who can replace the program, roll back
its state directory, or change the system clock. Standard OS account isolation
protects staged files; another process with the same user's authority is outside
this boundary. Metadata expiry bounds signed-metadata freeze attacks; release
discovery still relies on authenticated GitHub availability and ordering.

## Provisioning and signing

The bundled root pins `kitlane-release-2026-01`, provisioned in the maintainer's
OS credential vault with explicit approval on 2026-10-15. Reuse that credential
for local signing; do not generate a replacement during ordinary builds.

Run `cargo run --manifest-path app/Cargo.toml --bin sign-update -- keygen` once
on the maintainer machine. It refuses to replace an existing credential and
stores the 32-byte random seed in the OS credential vault using account
`kitlane.release-signing.v1` (existing Kitlane vault service). Its only output is
the public key JSON entry. Add that entry to `app/update-root.json` **before
building the binaries**. The private seed never belongs in Git, an artifact,
a plaintext key file, a command argument, or logs. `sign-update public` prints
only the existing public key.

Local signing reads the vault by default:

```powershell
cargo run --manifest-path app/Cargo.toml --bin sign-update -- sign dist/Kitlane-0.2.0-Setup-x64.exe 0.2.0 2000 dist/update-preview-v1.json
```

For CI, a maintainer must separately provision the same base64 32-byte seed as
the masked GitHub Actions secret `KITLANE_UPDATE_SIGNING_KEY` through an approved
secret-management channel. No application or repository automation exports the
vault secret. The publish job invokes the Node built-in-crypto signer with that
secret in the environment, checks that its public key is in the bundled root,
and publishes metadata alongside the five existing packages. Missing/wrong
secrets fail the job before publication. CI sequence is
`GITHUB_RUN_NUMBER * 1000 + GITHUB_RUN_ATTEMPT`; keep run attempts below 1000 and
never reset workflow numbering without advancing the sequence scheme. Reserve
local sequences below the next CI sequence, or deliberately advance CI beyond
any local/public release sequence. Published manifests must have unique
increasing sequences. Tag versions must match Cargo's package version.

Creating a local key or signed installer does not configure GitHub secrets or
publish a release. An existing unsigned installed release must be upgraded once
using a trusted installer containing this updater and public root. Already
published unsigned release assets cannot bootstrap signature trust.

## Rotation, revocation, and expiry

Rotate by shipping a release whose bundled root trusts both old and new public
keys, signed by the old key. After clients have installed that bridge release,
sign subsequent releases with the new key. A later release removes/revokes the
old key and raises `minimum_sequence`. A binary that has never installed the
bridge cannot trust a new key; provide a manual trusted installer for it.
Compromise of a currently trusted key cannot be safely repaired solely with
metadata signed by that key: distribute an independently authenticated installer
with the revoked key and higher root floor. There is no claimed online root
rotation or emergency revocation service.

Expired metadata is rejected even for a staged installer. Publish a newer
version with fresh signed metadata/sequence before expiry, or use a trusted
manual installer. Do not edit a signed payload, reuse a sequence for refreshed
metadata, weaken expiry checks, or trust a replacement checksum file.

References: [Dalek strict verification](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html#method.verify_strict),
[reqwest redirect policy](https://docs.rs/reqwest/0.13.5/reqwest/redirect/struct.Policy.html),
[Node signing API](https://nodejs.org/api/crypto.html#cryptosignalgorithm-data-key-callback),
[GitHub releases API](https://docs.github.com/en/rest/releases/releases#list-releases).
