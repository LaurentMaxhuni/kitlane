//! Opted-in, exact-ID WinGet upgrades. No installs, shell, or blanket upgrades.
// Non-Windows builds expose Unsupported; provider helpers remain for portable fixtures.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::model::DeviceInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramUpdateRequest {
    pub app_id: String,
    pub winget_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProgramUpdateOutcome {
    Available {
        old_version: String,
        new_version: String,
    },
    Updated {
        old_version: String,
        new_version: String,
        app_id: String,
        display_name: String,
    },
    NoUpdate {
        version: String,
    },
    NeedsUserAction {
        reason: ProgramUpdateReason,
        detail: String,
    },
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramUpdateReason {
    InvalidRequest,
    ProviderUnavailable,
    UntrustedProvider,
    UntrustedSource,
    InventoryUnavailable,
    NotInstalled,
    AmbiguousIdentity,
    UnknownVersion,
    Excluded,
    PinnedOrUnknownPins,
    Agreements,
    Elevation,
    Restart,
    TimedOut,
    VerificationFailed,
    ProviderFailed,
}

fn action(reason: ProgramUpdateReason, detail: impl Into<String>) -> ProgramUpdateOutcome {
    ProgramUpdateOutcome::NeedsUserAction {
        reason,
        detail: detail.into(),
    }
}

/// Run on the coordinator's worker thread: read-only provider probes may take 90s.
pub fn check_program_update(
    device: DeviceInfo,
    request: &ProgramUpdateRequest,
) -> ProgramUpdateOutcome {
    #[cfg(windows)]
    {
        windows::prepare(device, request).map_or_else(
            |outcome| outcome,
            |prepared| ProgramUpdateOutcome::Available {
                old_version: prepared.before.version.clone().unwrap_or_default(),
                new_version: prepared.target,
            },
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (device, request);
        ProgramUpdateOutcome::Unsupported
    }
}

/// Persist Running BEFORE calling this. Never call on the UI thread.
/// On an interruption retain the durable job and require reconciliation, never retry blindly.
pub fn start_program_update(
    device: DeviceInfo,
    request: &ProgramUpdateRequest,
) -> Result<RunningProgramUpdate, ProgramUpdateOutcome> {
    #[cfg(windows)]
    {
        windows::start(device, request, None)
    }
    #[cfg(not(windows))]
    {
        let _ = (device, request);
        Err(ProgramUpdateOutcome::Unsupported)
    }
}

/// Launch only the exact versions already recorded in the durable Running job.
/// Any provider or local-version change during preflight requires a new check job.
pub fn start_program_update_exact(
    device: DeviceInfo,
    request: &ProgramUpdateRequest,
    expected_old: &str,
    expected_new: &str,
) -> Result<RunningProgramUpdate, ProgramUpdateOutcome> {
    #[cfg(windows)]
    {
        windows::start(device, request, Some((expected_old, expected_new)))
    }
    #[cfg(not(windows))]
    {
        let _ = (device, request, expected_old, expected_new);
        Err(ProgramUpdateOutcome::Unsupported)
    }
}

/// Owns the live installer. Dropping this handle NEVER kills the installer.
pub struct RunningProgramUpdate {
    #[cfg(windows)]
    inner: windows::Running,
}

impl RunningProgramUpdate {
    pub fn old_version(&self) -> &str {
        #[cfg(windows)]
        {
            self.inner
                .prepared
                .before
                .version
                .as_deref()
                .unwrap_or_default()
        }
        #[cfg(not(windows))]
        {
            ""
        }
    }

    pub fn new_version(&self) -> &str {
        #[cfg(windows)]
        {
            &self.inner.prepared.target
        }
        #[cfg(not(windows))]
        {
            ""
        }
    }

    pub fn pid(&self) -> u32 {
        #[cfg(windows)]
        {
            self.inner.child.id()
        }
        #[cfg(not(windows))]
        {
            0
        }
    }

    /// None means still running. After exit, performs a fresh inventory scan;
    /// call on a worker thread because that scan can block. Terminal results are cached.
    /// Long-running installers remain live; coordinator may show elapsed time.
    pub fn poll(&mut self) -> Option<ProgramUpdateOutcome> {
        #[cfg(windows)]
        {
            self.inner.poll()
        }
        #[cfg(not(windows))]
        {
            Some(ProgramUpdateOutcome::Unsupported)
        }
    }
}

fn valid_winget_id(id: &str) -> bool {
    (3..=128).contains(&id.len())
        && id.contains('.')
        && id.split('.').all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_alphanumeric()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'+')
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SourceExport {
    name: String,
    arg: String,
    r#type: String,
    identifier: String,
    data: String,
    trust_level: Vec<String>,
}

fn validate_source_export(bytes: &[u8]) -> bool {
    if bytes.len() > 32 * 1024 {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<SourceExport>(bytes) else {
        return false;
    };
    value.name == "winget"
        && value.arg == "https://cdn.winget.microsoft.com/cache"
        && value.r#type == "Microsoft.PreIndexed.Package"
        && value.identifier == "Microsoft.Winget.Source_8wekyb3d8bbwe"
        && value.data == "Microsoft.Winget.Source_8wekyb3d8bbwe"
        && value.trust_level.len() == 2
        && value.trust_level.iter().any(|level| level == "Trusted")
        && value.trust_level.iter().any(|level| level == "StoreOrigin")
}

fn numeric_version(value: &str) -> Option<Vec<u64>> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return None;
    }
    let mut parts: Vec<u64> = value
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.len() > 12 {
        return None;
    }
    while parts.last() == Some(&0) {
        parts.pop();
    }
    Some(parts)
}

fn compare_versions(old: &str, new: &str) -> Option<std::cmp::Ordering> {
    let mut old = numeric_version(old)?;
    let mut new = numeric_version(new)?;
    let length = old.len().max(new.len());
    old.resize(length, 0);
    new.resize(length, 0);
    Some(old.cmp(&new))
}

/// Parse only an exact, untruncated provider row. Localized headers are irrelevant.
#[cfg(test)]
fn parse_available(output: &str, id: &str, installed: &str) -> Option<String> {
    parse_listing(output, id, installed).flatten()
}

fn parse_listing(output: &str, id: &str, installed: &str) -> Option<Option<String>> {
    let mut found = None;
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(index) = fields.iter().position(|field| *field == id) else {
            continue;
        };
        if index == 0
            || fields[index + 1..].len() < 2
            || fields[index + 1] != installed
            || found.is_some()
        {
            return None;
        }
        found = Some(match &fields[index + 1..] {
            [_, "winget"] => None,
            [_, target, "winget"]
                if compare_versions(installed, target) == Some(std::cmp::Ordering::Less) =>
            {
                Some((*target).into())
            }
            _ => return None,
        });
    }
    found
}

// Pin list has no machine-readable output. Positive no-pins evidence is deliberately
// conservative; any pins (including installed-product pins), localization or warning blocks.
fn no_pins(output: &str) -> bool {
    output.trim() == "There are no pins configured."
}

fn upgrade_args(id: &str, target: &str, scope: &str) -> Vec<String> {
    [
        "upgrade",
        "--id",
        id,
        "--exact",
        "--source",
        "winget",
        "--version",
        target,
        "--scope",
        scope,
        "--silent",
        "--disable-interactivity",
        "--authentication-mode",
        "silent",
        "--skip-dependencies",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn exit_reason(code: u32) -> ProgramUpdateReason {
    match code {
        0x8A150041 | 0x8A150042 | 0x8A150046 => ProgramUpdateReason::Agreements,
        0x8A150019 | 0x800702E4 | 740 | 5 | 0x80070005 => ProgramUpdateReason::Elevation,
        0x8A150109..=0x8A15010B | 3010 | 1641 => ProgramUpdateReason::Restart,
        0x8A150050 => ProgramUpdateReason::UnknownVersion,
        0x8A150068 => ProgramUpdateReason::PinnedOrUnknownPins,
        _ => ProgramUpdateReason::ProviderFailed,
    }
}

fn verify_versions(
    before: &crate::model::InstalledApp,
    after: &crate::model::InstalledApp,
    target: &str,
    code: u32,
) -> ProgramUpdateOutcome {
    // Both observations have already been uniquely associated with the requested
    // provider product. MSI upgrades can replace the key and versioned name.
    if before.publisher != after.publisher
        || before.source != after.source
        || before.scope != after.scope
    {
        return action(
            ProgramUpdateReason::VerificationFailed,
            "Installed product identity changed; review inventory before another update",
        );
    }
    let (Some(old), Some(new)) = (before.version.as_deref(), after.version.as_deref()) else {
        return action(
            ProgramUpdateReason::UnknownVersion,
            "Could not verify the installed version after update",
        );
    };
    // Restart/elevation/agreement results must remain visible even when inventory changed.
    if code != 0 && code != 0x8A15002B {
        return action(
            exit_reason(code),
            format!("WinGet returned 0x{code:08X}; observed version {old} → {new}"),
        );
    }
    if code == 0
        && compare_versions(old, new) == Some(std::cmp::Ordering::Less)
        && compare_versions(target, new) == Some(std::cmp::Ordering::Equal)
    {
        ProgramUpdateOutcome::Updated {
            old_version: old.into(),
            new_version: new.into(),
            app_id: after.id.clone(),
            display_name: after.name.clone(),
        }
    } else if code == 0x8A15002B && old == new {
        ProgramUpdateOutcome::NoUpdate {
            version: new.into(),
        }
    } else {
        action(
            ProgramUpdateReason::VerificationFailed,
            format!(
                "WinGet exited but installed version did not match the expected update ({old} → {new}; expected {target})"
            ),
        )
    }
}

fn inventory_warnings_allow_update(warnings: &[String]) -> bool {
    warnings.iter().all(|warning| warning == "Coverage: current-user and machine uninstall registry only. MSIX/Store apps, portable apps, other users, and unregistered apps are not scanned; registry entries may be stale."
        || ["HKCU 64-bit uninstall root is absent.", "HKCU 32-bit uninstall root is absent.", "HKLM 64-bit uninstall root is absent.", "HKLM 32-bit uninstall root is absent."].contains(&warning.as_str()))
}

fn select_installed(
    snapshot: &crate::model::InventorySnapshot,
    request: &ProgramUpdateRequest,
) -> Result<crate::model::InstalledApp, ProgramUpdateOutcome> {
    select_associated(snapshot, request, true)
}

fn select_associated(
    snapshot: &crate::model::InventorySnapshot,
    request: &ProgramUpdateRequest,
    require_original_registration: bool,
) -> Result<crate::model::InstalledApp, ProgramUpdateOutcome> {
    use crate::{
        catalog::{Catalog, MatchOutcome},
        model::{InstallScope, InventorySource},
    };
    if snapshot.validate().is_err() || !inventory_warnings_allow_update(&snapshot.warnings) {
        return Err(action(
            ProgramUpdateReason::InventoryUnavailable,
            "Inventory is invalid or partial; review scanner warnings before updating",
        ));
    }
    let catalog = Catalog::bundled().map_err(|_| {
        action(
            ProgramUpdateReason::AmbiguousIdentity,
            "Product catalog unavailable",
        )
    })?;
    let matches: Vec<_> = snapshot.apps.iter().filter(|app| matches!(catalog.match_app(app), MatchOutcome::Matched(product) if product.winget_id.as_deref() == Some(&request.winget_id))).collect();
    if matches.is_empty() {
        return Err(action(
            ProgramUpdateReason::NotInstalled,
            "Requested product is not installed; updates never install missing products",
        ));
    }
    if matches.len() != 1
        || (require_original_registration && matches[0].id != request.app_id)
        || matches[0].source != InventorySource::WindowsRegistry
    {
        return Err(action(
            ProgramUpdateReason::AmbiguousIdentity,
            "Provider ID does not identify exactly the selected installed application",
        ));
    }
    let app = matches[0];
    if app.version.as_deref().and_then(numeric_version).is_none() {
        return Err(action(
            ProgramUpdateReason::UnknownVersion,
            "Automatic updates require a known numeric installed version",
        ));
    }
    if app.scope == InstallScope::Unknown {
        return Err(action(
            ProgramUpdateReason::AmbiguousIdentity,
            "Installed scope is unknown",
        ));
    }
    Ok(app.clone())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::{
        inventory,
        model::{InstallScope, InstalledApp, Platform},
    };
    use std::{
        fs,
        io::Read,
        os::windows::{fs::MetadataExt, process::CommandExt},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::mpsc::{self, Receiver},
        thread,
        time::{Duration, Instant},
    };
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
    };

    const MAX_OUTPUT: usize = 256 * 1024;
    const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
    const REPARSE_POINT: u32 = 0x400;

    pub(super) struct Prepared {
        pub before: InstalledApp,
        pub target: String,
        executable: PathBuf,
        device: DeviceInfo,
        request: ProgramUpdateRequest,
    }

    fn observed(
        device: &DeviceInfo,
        request: &ProgramUpdateRequest,
    ) -> Result<InstalledApp, ProgramUpdateOutcome> {
        let snapshot = inventory::scan(device.clone()).map_err(|_| {
            action(
                ProgramUpdateReason::InventoryUnavailable,
                "Installed application inventory could not be read",
            )
        })?;
        select_installed(&snapshot, request)
    }

    fn regular_path(path: &Path, directory: bool) -> bool {
        fs::symlink_metadata(path).is_ok_and(|metadata| {
            metadata.file_attributes() & REPARSE_POINT == 0
                && if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                }
        })
    }

    fn protected_components(path: &Path) -> bool {
        use std::path::{Component, Prefix};
        matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
            && path.is_absolute()
            && !path
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
            && path
                .ancestors()
                .all(|component| regular_path(component, true))
    }

    fn package_version(name: &str) -> Option<Vec<u64>> {
        let inner = name
            .strip_prefix("Microsoft.DesktopAppInstaller_")?
            .strip_suffix("__8wekyb3d8bbwe")?;
        let (version, architecture) = inner.split_once('_')?;
        let expected = match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            "x86" => "x86",
            _ => return None,
        };
        if architecture != expected || version.split('.').count() != 4 {
            return None;
        }
        numeric_version(version)
    }

    fn executable() -> Result<PathBuf, ProgramUpdateOutcome> {
        let registry = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion",
                KEY_READ | KEY_WOW64_64KEY,
            )
            .map_err(|_| {
                action(
                    ProgramUpdateReason::ProviderUnavailable,
                    "Windows Program Files registration unavailable",
                )
            })?;
        let directory: String = registry.get_value("ProgramFilesDir").map_err(|_| {
            action(
                ProgramUpdateReason::ProviderUnavailable,
                "Windows Program Files registration unavailable",
            )
        })?;
        let root = PathBuf::from(directory).join("WindowsApps");
        if !protected_components(&root) {
            return Err(action(
                ProgramUpdateReason::UntrustedProvider,
                "Protected WindowsApps path unavailable or contains a reparse point",
            ));
        }
        // WindowsApps enumeration is denied to ordinary users on some systems.
        // Registration supplies only package names; executable authority still
        // comes from the protected root and exact Microsoft package identity.
        let names: Vec<String> = if let Ok(entries) = fs::read_dir(&root) {
            let mut names = Vec::new();
            for (index, entry) in entries.enumerate() {
                if index >= 8192 {
                    return Err(action(
                        ProgramUpdateReason::ProviderUnavailable,
                        "WindowsApps discovery exceeded its entry limit",
                    ));
                }
                let entry = entry.map_err(|_| {
                    action(
                        ProgramUpdateReason::ProviderUnavailable,
                        "WindowsApps directory could not be read completely",
                    )
                })?;
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_owned());
                }
            }
            names
        } else {
            let packages = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
                "Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppModel\\Repository\\Packages", KEY_READ
            ).map_err(|_| action(ProgramUpdateReason::ProviderUnavailable, "Microsoft App Installer registration is unavailable"))?;
            let mut names = Vec::new();
            for (index, name) in packages.enum_keys().enumerate() {
                if index >= 8192 {
                    return Err(action(
                        ProgramUpdateReason::ProviderUnavailable,
                        "App Installer registration exceeded its entry limit",
                    ));
                }
                names.push(name.map_err(|_| {
                    action(
                        ProgramUpdateReason::ProviderUnavailable,
                        "App Installer registration could not be read completely",
                    )
                })?);
            }
            names
        };
        let mut best: Option<(Vec<u64>, PathBuf)> = None;
        for name in names {
            let Some(version) = package_version(&name) else {
                continue;
            };
            let package = root.join(name);
            let exe = package.join("winget.exe");
            if regular_path(&package, true)
                && regular_path(&exe, false)
                && best.as_ref().is_none_or(|(current, _)| version > *current)
            {
                best = Some((version, exe));
            }
        }
        best.map(|(_, path)| path).ok_or_else(|| {
            action(
                ProgramUpdateReason::ProviderUnavailable,
                "Trusted Microsoft Desktop App Installer package was not found",
            )
        })
    }

    fn command(path: &Path, args: &[String]) -> Command {
        let mut command = Command::new(path);
        command
            .env_remove("TYPESAFE_API_KEY")
            .env_remove("KITLANE_TYPESAFE_API_KEY")
            .env_remove("KITLANE_UPDATE_SIGNING_KEY");
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000);
        command
    }

    // Drain continuously so a noisy child cannot block; retain at most MAX_OUTPUT.
    // A detached drain thread may survive inherited pipes; poll never waits on it.
    fn drain(mut reader: impl Read + Send + 'static) -> Receiver<Result<Vec<u8>, ()>> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut chunk = [0; 8192];
            let mut exceeded = false;
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        let remaining = MAX_OUTPUT.saturating_sub(bytes.len());
                        bytes.extend_from_slice(&chunk[..count.min(remaining)]);
                        exceeded |= count > remaining;
                    }
                    Err(_) => {
                        let _ = sender.send(Err(()));
                        return;
                    }
                }
            }
            let _ = sender.send(if exceeded { Err(()) } else { Ok(bytes) });
        });
        receiver
    }

    struct Probe {
        code: u32,
        stdout: Vec<u8>,
    }

    fn probe(path: &Path, args: &[&str]) -> Result<Probe, ProgramUpdateOutcome> {
        probe_with_timeout(path, args, CHECK_TIMEOUT)
    }

    fn probe_with_timeout(
        path: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<Probe, ProgramUpdateOutcome> {
        let args: Vec<_> = args.iter().map(|arg| (*arg).into()).collect();
        let mut child = command(path, &args).spawn().map_err(|_| {
            action(
                ProgramUpdateReason::ProviderUnavailable,
                "Could not launch trusted WinGet",
            )
        })?;
        let stdout = drain(child.stdout.take().unwrap());
        let stderr = drain(child.stderr.take().unwrap());
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(action(
                        ProgramUpdateReason::TimedOut,
                        "Read-only WinGet check timed out",
                    ));
                }
                Err(_) => {
                    return Err(action(
                        ProgramUpdateReason::ProviderFailed,
                        "Could not observe WinGet check",
                    ));
                }
            }
        };
        let output = stdout
            .recv_timeout(Duration::from_secs(1))
            .ok()
            .and_then(Result::ok);
        let errors = stderr
            .recv_timeout(Duration::from_secs(1))
            .ok()
            .and_then(Result::ok);
        match (output, errors) {
            (Some(stdout), Some(_)) => Ok(Probe {
                code: status.code().map_or(u32::MAX, |code| code as u32),
                stdout,
            }),
            _ => Err(action(
                ProgramUpdateReason::ProviderFailed,
                "WinGet output exceeded limits or could not be read completely",
            )),
        }
    }

    fn successful(output: Probe) -> Result<Vec<u8>, ProgramUpdateOutcome> {
        if output.code == 0 {
            Ok(output.stdout)
        } else {
            Err(action(
                exit_reason(output.code),
                format!("WinGet check returned 0x{:08X}", output.code),
            ))
        }
    }

    pub(super) fn prepare(
        device: DeviceInfo,
        request: &ProgramUpdateRequest,
    ) -> Result<Prepared, ProgramUpdateOutcome> {
        if device.platform != Platform::Windows
            || device.validate().is_err()
            || request.app_id.is_empty()
            || request.app_id.len() > 256
            || !valid_winget_id(&request.winget_id)
        {
            return Err(action(
                ProgramUpdateReason::InvalidRequest,
                "A valid installed application and exact WinGet ID are required",
            ));
        }
        if request.winget_id == "Microsoft.AppInstaller"
            || request.winget_id == "Microsoft.DesktopAppInstaller"
            || request.winget_id.to_ascii_lowercase().contains("kitlane")
        {
            return Err(action(
                ProgramUpdateReason::Excluded,
                "Provider and Kitlane updates require their dedicated update channels",
            ));
        }
        let before = observed(&device, request)?;
        let executable = executable()?;
        let source = successful(probe(
            &executable,
            &[
                "source",
                "export",
                "--name",
                "winget",
                "--disable-interactivity",
            ],
        )?)?;
        if !validate_source_export(&source) {
            return Err(action(
                ProgramUpdateReason::UntrustedSource,
                "WinGet source does not match the official Microsoft source configuration",
            ));
        }
        let pins = successful(probe(
            &executable,
            &[
                "pin",
                "list",
                "--source",
                "winget",
                "--disable-interactivity",
                "--authentication-mode",
                "silent",
            ],
        )?)?;
        if !std::str::from_utf8(&pins).is_ok_and(no_pins) {
            return Err(action(
                ProgramUpdateReason::PinnedOrUnknownPins,
                "WinGet pins exist or could not be interpreted safely; review pins manually",
            ));
        }
        let listing = successful(probe(
            &executable,
            &[
                "list",
                "--id",
                &request.winget_id,
                "--exact",
                "--source",
                "winget",
                "--disable-interactivity",
                "--authentication-mode",
                "silent",
            ],
        )?)?;
        let target = std::str::from_utf8(&listing)
            .ok()
            .and_then(|output| {
                parse_listing(
                    output,
                    &request.winget_id,
                    before.version.as_deref().unwrap_or_default(),
                )
            })
            .ok_or_else(|| {
                action(
                    ProgramUpdateReason::UnknownVersion,
                    "WinGet did not report exactly one installed version matching local inventory",
                )
            })?;
        let Some(target) = target else {
            return Err(ProgramUpdateOutcome::NoUpdate {
                version: before.version.clone().unwrap_or_default(),
            });
        };
        Ok(Prepared {
            before,
            target,
            executable,
            device,
            request: request.clone(),
        })
    }

    pub(super) fn start(
        device: DeviceInfo,
        request: &ProgramUpdateRequest,
        expected: Option<(&str, &str)>,
    ) -> Result<RunningProgramUpdate, ProgramUpdateOutcome> {
        let prepared = prepare(device, request)?;
        if expected.is_some_and(|(old, new)| {
            prepared.before.version.as_deref() != Some(old) || prepared.target != new
        }) {
            return Err(action(
                ProgramUpdateReason::VerificationFailed,
                "Installed or available version changed; record a fresh update check before retrying",
            ));
        }
        // Catch uninstall, replacement, or concurrent version changes during network checks.
        if observed(&prepared.device, request)? != prepared.before {
            return Err(action(
                ProgramUpdateReason::VerificationFailed,
                "Installed application changed during update checks",
            ));
        }
        if !prepared
            .executable
            .parent()
            .is_some_and(protected_components)
            || !regular_path(&prepared.executable, false)
        {
            return Err(action(
                ProgramUpdateReason::UntrustedProvider,
                "WinGet executable changed during checks",
            ));
        }
        let scope = match prepared.before.scope {
            InstallScope::User => "user",
            InstallScope::Machine => "machine",
            InstallScope::Unknown => unreachable!(),
        };
        let args = upgrade_args(&request.winget_id, &prepared.target, scope);
        let mut child = command(&prepared.executable, &args).spawn().map_err(|_| {
            action(
                ProgramUpdateReason::ProviderUnavailable,
                "Could not launch trusted WinGet upgrade",
            )
        })?;
        let stdout = drain(child.stdout.take().unwrap());
        let stderr = drain(child.stderr.take().unwrap());
        Ok(RunningProgramUpdate {
            inner: Running {
                child,
                prepared,
                stdout,
                stderr,
                result: None,
            },
        })
    }

    pub(super) struct Running {
        pub child: Child,
        pub prepared: Prepared,
        stdout: Receiver<Result<Vec<u8>, ()>>,
        stderr: Receiver<Result<Vec<u8>, ()>>,
        result: Option<ProgramUpdateOutcome>,
    }

    impl Running {
        pub fn poll(&mut self) -> Option<ProgramUpdateOutcome> {
            if let Some(result) = &self.result {
                return Some(result.clone());
            }
            let status = match self.child.try_wait() {
                Ok(None) => return None,
                Ok(Some(status)) => status,
                Err(_) => {
                    return Some(action(
                        ProgramUpdateReason::VerificationFailed,
                        "Lost installer process state; reconcile inventory and do not retry automatically",
                    ));
                }
            };
            // Output is not success authority. It is bounded and discarded; exit +
            // independently observed identity/version determine the outcome.
            let _ = self.stdout.try_recv();
            let _ = self.stderr.try_recv();
            let after = inventory::scan(self.prepared.device.clone())
                .map_err(|_| {
                    action(
                        ProgramUpdateReason::InventoryUnavailable,
                        "Cannot re-inventory updated product",
                    )
                })
                .and_then(|snapshot| select_associated(&snapshot, &self.prepared.request, false));
            let result = match after {
                Ok(after) => verify_versions(
                    &self.prepared.before,
                    &after,
                    &self.prepared.target,
                    status.code().map_or(u32::MAX, |code| code as u32),
                ),
                Err(_) => action(
                    ProgramUpdateReason::VerificationFailed,
                    "Installed product could not be independently verified after WinGet exited; review inventory before retrying",
                ),
            };
            self.result = Some(result.clone());
            Some(result)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::io::Write;

        #[test]
        fn package_discovery_rejects_user_alias_and_publisher_architecture_confusion() {
            let arch = match std::env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                "x86" => "x86",
                _ => return,
            };
            assert!(
                package_version(&format!(
                    "Microsoft.DesktopAppInstaller_1.2.3.4_{arch}__8wekyb3d8bbwe"
                ))
                .is_some()
            );
            for name in [
                format!("Microsoft.DesktopAppInstaller_1.2.3.4_{arch}__attacker"),
                format!("Microsoft.DesktopAppInstaller_latest_{arch}__8wekyb3d8bbwe"),
                "winget.exe".into(),
                "Microsoft.DesktopAppInstaller_1.2.3.4_other__8wekyb3d8bbwe".into(),
            ] {
                assert!(package_version(&name).is_none());
            }
            assert!(!protected_components(Path::new(
                r"\\attacker\share\WindowsApps"
            )));
            assert!(!protected_components(Path::new(r"C:\Windows\..\Users")));
        }

        // Spawned only as a test executable fixture, never an installed application.
        #[test]
        fn mock_probe_child() {
            thread::sleep(Duration::from_millis(200));
            std::io::stdout()
                .write_all(&vec![b'x'; MAX_OUTPUT + 1])
                .unwrap();
        }

        #[test]
        fn probes_bound_output_and_read_only_wait_time() {
            let executable = std::env::current_exe().unwrap();
            let args = [
                "--exact",
                "program_updates::windows::tests::mock_probe_child",
                "--nocapture",
            ];
            let output = probe_with_timeout(&executable, &args, Duration::from_secs(10));
            assert!(matches!(
                output,
                Err(ProgramUpdateOutcome::NeedsUserAction {
                    reason: ProgramUpdateReason::ProviderFailed,
                    ..
                })
            ));
            let started = Instant::now();
            let output = probe_with_timeout(&executable, &args, Duration::from_millis(25));
            assert!(matches!(
                output,
                Err(ProgramUpdateOutcome::NeedsUserAction {
                    reason: ProgramUpdateReason::TimedOut,
                    ..
                })
            ));
            assert!(started.elapsed() < Duration::from_secs(5));
        }

        #[test]
        #[ignore = "read-only installed Microsoft App Installer/source discovery; run explicitly on Windows"]
        fn read_only_provider_discovery() {
            let path = executable().expect("trusted installed Microsoft App Installer executable");
            eprintln!("Resolved protected provider: {}", path.display());
            let source = successful(
                probe(
                    &path,
                    &[
                        "source",
                        "export",
                        "--name",
                        "winget",
                        "--disable-interactivity",
                    ],
                )
                .expect("read-only provider launch"),
            )
            .expect("read-only source export");
            assert!(validate_source_export(&source), "official source tuple");
            eprintln!("Official Microsoft WinGet source configuration validated");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_argument_and_identity_confusion() {
        for id in ["--all", "A B", "A;B", "A\nB", "", "Publisher.*", ".Bad"] {
            assert!(!valid_winget_id(id), "{id:?}");
        }
        assert!(valid_winget_id("Microsoft.VisualStudioCode"));
        assert!(valid_winget_id("Notepad++.Notepad++"));
    }

    #[test]
    fn source_name_alone_is_not_authority() {
        let valid = r#"{"Name":"winget","Arg":"https://cdn.winget.microsoft.com/cache","Type":"Microsoft.PreIndexed.Package","Identifier":"Microsoft.Winget.Source_8wekyb3d8bbwe","Data":"Microsoft.Winget.Source_8wekyb3d8bbwe","TrustLevel":["Trusted","StoreOrigin"]}"#;
        assert!(validate_source_export(valid.as_bytes()));
        for (old, new) in [
            (
                "https://cdn.winget.microsoft.com/cache",
                "https://evil.test/cache",
            ),
            ("Microsoft.PreIndexed.Package", "Microsoft.Rest"),
            ("Microsoft.Winget.Source_8wekyb3d8bbwe", "Other.Source"),
            ("winget", "other"),
        ] {
            assert!(!validate_source_export(valid.replace(old, new).as_bytes()));
        }
        assert!(!validate_source_export(
            format!("{valid}\n{valid}").as_bytes()
        ));
        assert!(!validate_source_export(
            valid
                .replacen(
                    "\"Name\":\"winget\"",
                    "\"Name\":\"evil\",\"Name\":\"winget\"",
                    1
                )
                .as_bytes()
        ));
    }

    #[test]
    fn refuses_unknown_and_non_increasing_versions() {
        assert_eq!(
            compare_versions("1.2", "1.3"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            compare_versions("1.2", "1.2.0"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(compare_versions("Unknown", "1.3"), None);
        assert_eq!(compare_versions("1.2-beta", "1.3"), None);
    }

    #[test]
    fn discovery_requires_exact_unambiguous_installed_and_target_versions() {
        let row = "Name Id Version Available Source\n--------------------------------\nEditor Microsoft.VisualStudioCode 1.2 1.3 winget\n";
        assert_eq!(
            parse_available(row, "Microsoft.VisualStudioCode", "1.2"),
            Some("1.3".into())
        );
        assert_eq!(parse_available(row, "Microsoft.VisualStudio", "1.2"), None);
        assert_eq!(
            parse_available(row, "Microsoft.VisualStudioCode", "1.1"),
            None
        );
        assert_eq!(
            parse_available(&format!("{row}{row}"), "Microsoft.VisualStudioCode", "1.2"),
            None
        );
    }

    #[test]
    fn pin_guard_requires_positive_no_pins_evidence() {
        assert!(no_pins("There are no pins configured.\r\n"));
        for output in [
            "",
            "No packages found",
            "Name Id Pin\nA Publisher.App Pinning",
            "There are no pins configured.\nUnexpected warning",
        ] {
            assert!(!no_pins(output));
        }
    }

    #[test]
    fn no_update_requires_installed_provider_row() {
        assert_eq!(
            parse_listing(
                "Editor Microsoft.VisualStudioCode 1.2 winget",
                "Microsoft.VisualStudioCode",
                "1.2"
            ),
            Some(None)
        );
        for output in [
            "No installed packages found",
            "Editor Microsoft.VisualStudioCode",
            "Editor Microsoft.VisualStudioCode 1.2",
            "Editor Microsoft.VisualStudioCode 1.2 msstore",
        ] {
            assert_eq!(
                parse_listing(output, "Microsoft.VisualStudioCode", "1.2"),
                None
            );
        }
    }

    #[test]
    fn upgrade_arguments_are_narrow_and_do_not_bypass_user_controls() {
        let args = upgrade_args("Microsoft.VisualStudioCode", "1.3", "user");
        assert_eq!(
            &args[..8],
            [
                "upgrade",
                "--id",
                "Microsoft.VisualStudioCode",
                "--exact",
                "--source",
                "winget",
                "--version",
                "1.3"
            ]
        );
        for forbidden in [
            "--all",
            "--force",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--allow-reboot",
            "--include-pinned",
            "--include-unknown",
            "--override",
            "--custom",
            "install",
        ] {
            assert!(!args.iter().any(|arg| arg == forbidden));
        }
        assert!(args.iter().any(|arg| arg == "--skip-dependencies"));
    }

    fn app(version: &str) -> crate::model::InstalledApp {
        use crate::model::{InstallScope, InventorySource};
        crate::model::InstalledApp {
            id: "selected".into(),
            name: "Editor".into(),
            publisher: Some("Publisher".into()),
            version: Some(version.into()),
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: Some("code".into()),
            origin: None,
        }
    }

    #[test]
    fn verified_upgrade_can_change_registration_and_versioned_display_name() {
        let mut after = app("1.3");
        after.id = "new-msi-registration".into();
        after.name = "Editor 1.3".into();
        after.native_id = Some("new-product-code".into());
        assert!(matches!(
            verify_versions(&app("1.2"), &after, "1.3", 0),
            ProgramUpdateOutcome::Updated { .. }
        ));
    }

    #[test]
    fn success_requires_observed_exact_identity_and_expected_newer_version() {
        assert!(matches!(
            verify_versions(&app("1.2"), &app("1.3"), "1.3", 0),
            ProgramUpdateOutcome::Updated { .. }
        ));
        assert!(matches!(
            verify_versions(&app("1.2"), &app("1.2"), "1.3", 0),
            ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::VerificationFailed,
                ..
            }
        ));
        assert!(matches!(
            verify_versions(&app("1.2"), &app("1.4"), "1.3", 0),
            ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::VerificationFailed,
                ..
            }
        ));
        let mut replaced = app("1.3");
        replaced.publisher = Some("Other".into());
        assert!(matches!(
            verify_versions(&app("1.2"), &replaced, "1.3", 0),
            ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::VerificationFailed,
                ..
            }
        ));
        assert!(matches!(
            verify_versions(&app("1.2"), &app("1.3"), "1.3", 3010),
            ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::Restart,
                ..
            }
        ));
        assert!(matches!(
            verify_versions(&app("1.2"), &app("1.3"), "1.3", 0x8A150041),
            ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::Agreements,
                ..
            }
        ));
    }

    #[test]
    fn changed_registration_still_requires_one_exact_provider_association() {
        use crate::model::{InventorySnapshot, Platform};
        let mut updated = app("1.3");
        updated.id = "new-registration".into();
        updated.name = "Microsoft Visual Studio Code".into();
        updated.publisher = Some("Microsoft Corporation".into());
        updated.native_id = None;
        let mut snapshot = InventorySnapshot {
            device: DeviceInfo {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Fixture".into(),
                platform: Platform::Windows,
                architecture: "x86_64".into(),
            },
            scanned_at_unix_secs: 1,
            apps: vec![updated.clone()],
            warnings: vec![],
        };
        let request = ProgramUpdateRequest {
            app_id: "old-registration".into(),
            winget_id: "Microsoft.VisualStudioCode".into(),
        };
        assert!(select_installed(&snapshot, &request).is_err());
        assert_eq!(
            select_associated(&snapshot, &request, false),
            Ok(updated.clone())
        );
        updated.id = "second-registration".into();
        snapshot.apps.push(updated);
        assert!(select_associated(&snapshot, &request, false).is_err());
    }

    #[test]
    fn inventory_requires_selected_existing_unique_product_and_complete_observations() {
        use crate::model::{InventorySnapshot, Platform};
        let mut installed = app("1.2");
        installed.name = "Microsoft Visual Studio Code".into();
        installed.publisher = Some("Microsoft Corporation".into());
        installed.native_id = None;
        let mut snapshot = InventorySnapshot {
            device: DeviceInfo {
                id: "50a4c2ed-0f60-4691-ae3a-8195bc3fa85a".into(),
                name: "Fixture".into(),
                platform: Platform::Windows,
                architecture: "x86_64".into(),
            },
            scanned_at_unix_secs: 1,
            apps: vec![installed.clone()],
            warnings: vec!["HKCU 32-bit uninstall root is absent.".into()],
        };
        let request = ProgramUpdateRequest {
            app_id: installed.id.clone(),
            winget_id: "Microsoft.VisualStudioCode".into(),
        };
        assert_eq!(select_installed(&snapshot, &request), Ok(installed.clone()));
        snapshot.warnings.push("HKLM registry access denied".into());
        assert!(matches!(
            select_installed(&snapshot, &request),
            Err(ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::InventoryUnavailable,
                ..
            })
        ));
        snapshot.warnings.clear();
        snapshot.apps.clear();
        assert!(matches!(
            select_installed(&snapshot, &request),
            Err(ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::NotInstalled,
                ..
            })
        ));
        let mut other = installed.clone();
        other.id = "other".into();
        snapshot.apps = vec![installed, other];
        assert!(matches!(
            select_installed(&snapshot, &request),
            Err(ProgramUpdateOutcome::NeedsUserAction {
                reason: ProgramUpdateReason::AmbiguousIdentity,
                ..
            })
        ));
    }
}
