# Linux inventory

`inventory_linux::scan` observes current-user and default-system Flatpak applications. Each scope runs separately; records carry `source: flatpak` and explicit `user`/`machine` scope. `native_id` is the complete observed application/architecture/branch ref (bounded to 256 bytes; longer refs retain the app with no native identifier), `origin` is observed remote name, version comes from installed deployment metadata, and publisher remains absent. Origin names are not verified URLs or publisher identity. No Flatpak installation, update, bootstrap, elevation, or network request is initiated.

Exact command argument vectors, without a shell:

```text
/usr/bin/flatpak list --app --user --columns=name:full,application:full,version:full,origin:full,ref:full
/usr/bin/flatpak list --app --system --columns=name:full,application:full,version:full,origin:full,ref:full
```

The absolute executable and its parent directories must be root-owned, not symlinks, and not group/world writable. Executable must be a regular executable file. Checks assume a trustworthy local administrator; stdlib pathname checks cannot eliminate replacement races by root. Environment is cleared except HOME and XDG installation-location configuration; locale and helper PATH are fixed, loader overrides removed, stdin closed, working directory `/`. Unsupported package-manager locations produce an explicit unavailable-source error; manager discovery/bootstrap is deliberately absent.

A pipe gives Flatpak headerless tab-separated output. Exactly five columns, UTF-8, metadata lengths, control characters, and ID/ref agreement are validated. `ref` contains application ID, architecture, and branch. UUIDv5 record IDs include scope, complete ref, and origin; branch/architecture variants remain separate even though shared model exposes no per-app architecture field. Catalog matching accepts only full stable refs, so beta and legacy ID-only snapshots remain unknown until an appropriate rescan. Only exact duplicate rows collapse; conflicting duplicates invalidate that scope. A single final newline from an empty table is accepted as empty output. Empty name falls back to application ID; empty version/origin remain absent.

Each subprocess has a 10-second deadline, 8 MiB stdout limit, 64 KiB stderr limit, and inventory has a 10,000-row/application limit. Linux procfs permits reopening owned pipe descriptors with O_NONBLOCK using safe stdlib calls; missing procfs fails explicitly. Streams are polled concurrently without blocking reader threads or unbounded `Command::output`. Every return kills/reaps the direct child; no arbitrary helper or app is executed. Kernel-level uninterruptible process cleanup cannot guarantee a wall-clock return deadline. Nonzero exit, any stderr diagnostics, malformed rows, inaccessible pipes, timeout, or oversized output invalidate that scope. One successful scope yields a fixed coverage warning; both failures return an error. Raw process errors and stderr are never emitted into snapshot warnings.

Coverage excludes runtimes, named system installations, other users, distribution packages, AppImages, portable apps, and unregistered apps. Successful empty inventory means only that the two selected scopes reported no applications.

Fixture tests run on Windows and cover malformed/truncated columns, UTF-8, controls, metadata bounds, row/byte limits, optional fields, duplicates, variant IDs, partial coverage, and bounded stream reads. The native Linux code also passes Clippy cross-checking for x86_64-unknown-linux-gnu. Native executable trust, procfs pipe polling, process exit/timeout cleanup, and real Flatpak output still require runtime validation on Linux; unavailable in this Windows development environment.

Sources checked 2026-10-11: [official Flatpak command reference](https://docs.flatpak.org/en/latest/flatpak-command-reference.html#flatpak-list), [official Flatpak list implementation](https://github.com/flatpak/flatpak/blob/main/app/flatpak-builtins-list.c). Documentation establishes `--app`, scope flags, columns, and full column values; implementation confirms partial refs and diagnostics when deployments cannot be inspected.

[Official table-printer implementation](https://github.com/flatpak/flatpak/blob/main/app/flatpak-table-printer.c) confirms tab-separated output when not attached to a terminal and a final newline for empty tables.
