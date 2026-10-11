# Windows inventory coverage

Phase 1 reads `Software\Microsoft\Windows\CurrentVersion\Uninstall` under HKCU and HKLM with `KEY_READ`, using separate `KEY_WOW64_64KEY` and `KEY_WOW64_32KEY` passes. Child opens retain the selected view. HKCU represents the current process user; other user hives are not enumerated. No registry writes, WMI, commands, elevation, or network requests execute during scanning.

MSIX/Store applications, portable software, unregistered installs, and other users are outside this pass. Registry entries can remain after removal and are not verified installation or catalog identity. On ARM64, the selected 32-bit view can omit ARM32 registrations. Every successful snapshot includes a coverage warning.

Missing roots are reported separately from unreadable roots. Partial failures warn; complete access failure returns `RegistryUnavailable`. All absent roots can produce an empty snapshot with explicit warnings. Non-Windows scanning returns `UnsupportedPlatform`. Device metadata and output records are validated; zero/pre-epoch timestamps fail. Scanning stops with an error beyond 40,000 visited keys or 10,000 retained applications. Warnings are bounded to 64. Entry value sizes are queried before reads; values above 16 KiB are rejected. A concurrent writer can still enlarge a value between querying its size and the safe winreg read; snapshots are observations rather than atomic registry transactions.

`SystemComponent=1`, nonempty parent metadata, and recognized update `ReleaseType` values exclude hidden components, children, and updates. Product names alone never trigger exclusion. Malformed UTF-16, types, controls, and oversized metadata warn and skip entries; empty optional strings become absent fields.

Deduplication requires the same hive scope, registry key, display metadata, and matching private installation evidence in opposite views: an identical uninstall command or an MSI GUID registration. Install location alone does not establish identity. Distinct scopes, keys, versions, or evidence remain distinct. IDs are UUID v5 values derived from scope, selected view, and key; display/version changes do not change IDs. The 64-bit observation supplies the merged ID. If a duplicate's 64-bit registration disappears, the surviving 32-bit ID changes. Registry paths, install locations, and uninstall commands are not emitted in snapshots or diagnostics. Commands are read only as matching evidence and never executed.

Validation uses pure fixture tests plus an explicitly ignored read-only local-registry integration test. Run the latter on Windows with:

```text
cargo test --manifest-path app/Cargo.toml -p kitlane-core reads_current_user_and_machine_registry_without_writes -- --ignored --nocapture
```

Sources checked 2026-10-11: [winreg 0.56 RegKey API](https://docs.rs/winreg/0.56.0/winreg/reg_key/struct.RegKey.html), [Microsoft uninstall metadata](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key), [alternate registry views](https://learn.microsoft.com/en-us/windows/win32/winprog64/accessing-an-alternate-registry-view), [SystemComponent semantics](https://learn.microsoft.com/en-us/windows/win32/msi/adding-and-removing-an-application-and-leaving-no-trace-in-the-registry), [parent update metadata](https://learn.microsoft.com/en-us/windows/win32/shell/cpl-setprogramaccess), and [Microsoft warning that Win32_Product may verify and repair installs](https://learn.microsoft.com/en-us/powershell/scripting/samples/working-with-software-installations).
