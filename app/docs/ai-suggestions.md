# AI alternatives

Kitlane uses TypeSafe Jev for optional, user-requested software recommendations.
The app never scans or sends the full inventory to this provider. An explicit
request sends the selected app's name and publisher, target OS, user preferences,
and a short list of documented candidate capabilities. Avoid putting personal
information in the free-text priorities field.

## Credentials

Set `TYPESAFE_API_KEY` in the process environment or save a key through the app.
Saved keys use the existing operating-system credential store, under the distinct
account `kitlane.ai.typesafe.v1`; no key is written to app JSON or the database.
A saved key takes precedence over an environment key. Forgetting a saved key does
not remove an environment key. Credential-store failures are surfaced explicitly.
Error messages never include HTTP bodies, authorization headers or raw vault
errors. In-memory owned key strings use `Zeroizing`.

## How suggestions work

The bundled `core/catalog/alternatives.json` contains 40 reviewed desktop products
across 16 categories: browsers, code editors, media players, notes, office suites,
PDF, CAD/3D, archives, search/utilities, messaging, Git clients, containers, image
editing, video editing, password managers and audio editing. Each entry contains
official source links, a review date, desktop platform information and narrowly
stated capabilities. Free means the desktop application or free tier; optional
services and advanced features may require payment. Open source refers to the
distributed candidate, so Microsoft's VS Code binary is excluded by that filter.
These entries establish candidate coverage, not exhaustive application coverage.

Reviewed source app associations retrieve candidates locally. Other selected
apps use one Jev Choice question to select a category, including an explicit
`none` outcome. Unknown identities, drivers, runtimes, games and unsupported
purposes must abstain. Code validates every option and probability and requires
category confidence of at least 0.6 and selected probability of at least 0.7.
These conservative thresholds are application policy, not model guarantees.

Code enforces target platform, free-only and open-source-only constraints before
scoring. A second request asks one independent Jev Score question for each eligible
candidate, in a single batch. Known source categories skip the routing request.
Every score uses the same three concrete levels: unsuitable/insufficient evidence,
plausible with compromises, and suitable on documented evidence.

Responses must contain exactly the requested candidate IDs, matching question
types and legends, finite bounded scores/confidence/probabilities, distributions
that sum to one, and scores consistent with their probability-weighted means.
Invalid responses fail closed. The app drops candidates below the middle rubric
level or with at least half their probability on unsuitable. It displays at most
three candidates in stable score order. Empty output is a valid abstention.

The displayed score is a normalized rubric position, not probability of software
quality. Confidence describes distribution concentration. Explanations come from
reviewed catalog facts; Jev cannot create source URLs, prices, product identities,
installer URLs or executable commands. Compatibility details, file-format parity,
migration, account requirements and individual workflows still need review.
Accepting a recommendation records a user decision; it does not authorize or run
an installation, removal or update.

## Network behavior

The client uses only `https://api.typesafe.ai/v1/systemone`, `jev-latest`, validated
Bearer credentials, a sensitive authorization header, HTTPS-only transport and
disabled redirects. Connect/request timeouts are 10/45 seconds. Each response is
bounded to 128 KiB. There are no automatic retries; failures require a new explicit
request. Category routing can add one request for an unfamiliar application.

Implementation follows the current [HTTP API](https://docs.typesafe.ai/api),
[Choice](https://docs.typesafe.ai/primitives/choice),
[Score](https://docs.typesafe.ai/primitives/score), and
[reranking cookbook](https://docs.typesafe.ai/cookbooks/rerank_typesafe), reviewed
2026-10-15. API responses remain judgments rather than verified product claims.

## Verification

Run offline unit checks:

```powershell
cargo test --offline --manifest-path app/Cargo.toml -p kitlane-core suggestions
cargo test --offline --manifest-path app/Cargo.toml --bin kitlane ai_client::tests::key_validation_and_os_vault_lifecycle
```

An ignored live diagnostic uses four synthetic examples only: a privacy browser
alternative, engineering CAD, offline office tools and an incompatible browser
request that should abstain. It does not read the installed inventory or write
credentials. It incurs provider usage and requires an environment key:

```powershell
cargo test --offline --manifest-path app/Cargo.toml --bin kitlane ai_client::tests::live_representative_alternatives -- --ignored --nocapture
```

Review representative cases after changing rubric wording, model behavior or
catalog coverage; four cases are a smoke check, not a quality benchmark.
