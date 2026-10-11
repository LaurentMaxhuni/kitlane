//! Bounded read-only inventory of current-user and default-system Flatpak apps.
use crate::inventory::InventoryError;
use crate::model::{DeviceInfo, InventorySnapshot};

pub fn scan(device: DeviceInfo) -> Result<InventorySnapshot, InventoryError> {
    device.validate().map_err(InventoryError::InvalidDevice)?;
    #[cfg(target_os = "linux")]
    {
        if device.platform != crate::model::Platform::Linux {
            return Err(InventoryError::InvalidDevice(
                "Device platform does not match Linux scanner",
            ));
        }
        native::scan(device)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(InventoryError::UnsupportedPlatform)
    }
}

#[cfg(any(target_os = "linux", test))]
mod records {
    use super::*;
    use crate::model::{InstallScope, InstalledApp, InventorySource};
    use std::collections::BTreeMap;
    use std::io::{self, Read};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) const MAX_STDOUT: usize = 8 * 1024 * 1024;
    pub(super) const MAX_STDERR: usize = 64 * 1024;
    const MAX_APPS: usize = 10_000;
    #[cfg(target_os = "linux")]
    pub(super) const COLUMNS: &str =
        "--columns=name:full,application:full,version:full,origin:full,ref:full";
    const COVERAGE: &str = "Coverage: current-user and default-system Flatpak applications only. Runtimes, named system installations, other users, native packages, AppImages, and portable apps are not scanned. Flatpak metadata and origin names are observations, not verified publisher identity.";

    pub(super) fn failure(message: &'static str) -> InventoryError {
        InventoryError::ScanFailed(message.into())
    }

    fn field(value: &str, max: usize, required: bool) -> Result<Option<String>, InventoryError> {
        if value.len() > max || value.chars().any(char::is_control) || value != value.trim() {
            return Err(failure("Flatpak returned invalid application metadata"));
        }
        if value.is_empty() {
            return if required {
                Err(failure("Flatpak returned missing application metadata"))
            } else {
                Ok(None)
            };
        }
        Ok(Some(value.into()))
    }

    pub(super) fn parse(
        bytes: &[u8],
        scope: InstallScope,
    ) -> Result<Vec<InstalledApp>, InventoryError> {
        if bytes.len() > MAX_STDOUT {
            return Err(InventoryError::LimitExceeded);
        }
        if !matches!(scope, InstallScope::User | InstallScope::Machine) {
            return Err(failure("Flatpak scan has an invalid installation scope"));
        }
        let output =
            std::str::from_utf8(bytes).map_err(|_| failure("Flatpak returned invalid UTF-8"))?;
        // The upstream table printer emits one final newline even with no rows.
        if output.is_empty() || output == "\n" {
            return Ok(Vec::new());
        }
        let mut apps = BTreeMap::new();
        let mut rows = 0;
        for line in output.split_terminator('\n') {
            rows += 1;
            if rows > MAX_APPS {
                return Err(InventoryError::LimitExceeded);
            }
            // Flatpak writes tab-separated, headerless rows when stdout is a pipe.
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 5 {
                return Err(failure("Flatpak returned an invalid column layout"));
            }
            let name = field(fields[0], 512, false)?;
            let application = field(fields[1], 256, true)?.unwrap();
            let version = field(fields[2], 128, false)?;
            let origin = field(fields[3], 256, false)?;
            let reference = field(fields[4], 512, true)?.unwrap();
            let parts: Vec<_> = reference.split('/').collect();
            if parts.len() != 3
                || parts[0] != application
                || !parts.iter().all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                })
                || !application.contains('.')
            {
                return Err(failure("Flatpak returned an invalid application ref"));
            }
            let scope_key = if scope == InstallScope::User {
                "user"
            } else {
                "system"
            };
            // Length-prefix origin so arbitrary remote names cannot collide with ref keys.
            let remote = origin.as_deref().unwrap_or("");
            let key = format!(
                "kitlane:flatpak:{scope_key}:{reference}:{}:{remote}",
                remote.len()
            );
            let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, key.as_bytes()).to_string();
            let app = InstalledApp {
                id: id.clone(),
                name: name.unwrap_or_else(|| application.clone()),
                publisher: None,
                version,
                scope,
                source: InventorySource::Flatpak,
                native_id: Some(application),
                origin,
            };
            if apps.get(&id).is_some_and(|existing| existing != &app) {
                return Err(failure(
                    "Flatpak returned conflicting duplicate application records",
                ));
            }
            apps.insert(id, app);
        }
        Ok(apps.into_values().collect())
    }

    pub(super) fn snapshot(
        device: DeviceInfo,
        results: [Result<Vec<InstalledApp>, InventoryError>; 2],
    ) -> Result<InventorySnapshot, InventoryError> {
        let mut apps = Vec::new();
        let mut warnings = vec![COVERAGE.into()];
        let mut succeeded = 0;
        for (result, scope) in results.into_iter().zip(["Current-user", "Default-system"]) {
            match result {
                Ok(mut records) => {
                    succeeded += 1;
                    apps.append(&mut records);
                }
                Err(_) => warnings.push(format!(
                    "{scope} Flatpak inventory could not be read completely; coverage is partial."
                )),
            }
        }
        if succeeded == 0 {
            return Err(failure(
                "Neither Flatpak installation scope could be read completely",
            ));
        }
        if apps.len() > MAX_APPS {
            return Err(InventoryError::LimitExceeded);
        }
        apps.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        let snapshot = InventorySnapshot {
            device,
            scanned_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| InventoryError::InvalidClock)?
                .as_secs(),
            apps,
            warnings,
        };
        snapshot
            .validate()
            .map_err(InventoryError::InvalidSnapshot)?;
        Ok(snapshot)
    }

    // One bounded read per polling cycle; never allocates beyond the stream's limit.
    pub(super) fn read_chunk(
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<(bool, bool), InventoryError> {
        let mut buffer = [0; 16 * 1024];
        match reader.read(&mut buffer) {
            Ok(0) => Ok((true, false)),
            Ok(count) => {
                if count > limit.saturating_sub(output.len()) {
                    return Err(InventoryError::LimitExceeded);
                }
                output.extend_from_slice(&buffer[..count]);
                Ok((false, true))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok((false, false)),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok((false, true)),
            Err(_) => Err(failure("Flatpak output could not be read")),
        }
    }
}

#[cfg(target_os = "linux")]
mod native {
    use super::records::*;
    use super::*;
    use crate::model::InstallScope;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    const EXECUTABLE: &str = "/usr/bin/flatpak";
    const DEADLINE: Duration = Duration::from_secs(10);

    fn trusted_executable() -> Result<(), InventoryError> {
        for path in ["/", "/usr", "/usr/bin", EXECUTABLE] {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| {
                InventoryError::SourceUnavailable("Trusted Flatpak executable is unavailable")
            })?;
            if metadata.file_type().is_symlink()
                || metadata.uid() != 0
                || metadata.mode() & 0o022 != 0
                || (path == EXECUTABLE && (!metadata.is_file() || metadata.mode() & 0o111 == 0))
                || (path != EXECUTABLE && !metadata.is_dir())
            {
                return Err(InventoryError::SourceUnavailable(
                    "Flatpak executable is not trusted",
                ));
            }
        }
        Ok(())
    }

    struct ManagedChild(Child);
    impl Drop for ManagedChild {
        fn drop(&mut self) {
            // Also covers errors opening/reading pipes and deadline/output-limit failures.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn nonblocking_pipe(pipe: &impl AsRawFd) -> Result<File, InventoryError> {
        // Linux O_NONBLOCK (octal 04000). Reopening our own procfs pipe descriptor
        // creates a nonblocking reader without unsafe fcntl or background threads.
        OpenOptions::new()
            .read(true)
            .custom_flags(0o4000)
            .open(format!("/proc/self/fd/{}", pipe.as_raw_fd()))
            .map_err(|_| failure("Flatpak output pipes require accessible procfs"))
    }

    fn capture(scope: InstallScope) -> Result<Vec<crate::model::InstalledApp>, InventoryError> {
        let scope_arg = if scope == InstallScope::User {
            "--user"
        } else {
            "--system"
        };
        let mut command = Command::new(EXECUTABLE);
        command
            .args(["list", "--app", scope_arg, COLUMNS])
            .current_dir("/")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("GIO_USE_VFS", "local")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Preserve only installation-location configuration; no loader, D-Bus,
        // manager override, or shell environment is forwarded.
        for name in [
            "HOME",
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "XDG_CONFIG_DIRS",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let started = Instant::now();
        let mut child = ManagedChild(
            command
                .spawn()
                .map_err(|_| failure("Flatpak inventory process could not start"))?,
        );
        let stdout_pipe = child
            .0
            .stdout
            .take()
            .ok_or_else(|| failure("Flatpak stdout is unavailable"))?;
        let stderr_pipe = child
            .0
            .stderr
            .take()
            .ok_or_else(|| failure("Flatpak stderr is unavailable"))?;
        let mut stdout = nonblocking_pipe(&stdout_pipe)?;
        let mut stderr = nonblocking_pipe(&stderr_pipe)?;
        drop((stdout_pipe, stderr_pipe));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (mut out_eof, mut err_eof) = (false, false);
        let mut status = None;
        loop {
            if started.elapsed() >= DEADLINE {
                return Err(failure("Flatpak inventory exceeded its time limit"));
            }
            let mut progress = false;
            if !out_eof {
                let result = read_chunk(&mut stdout, &mut out, MAX_STDOUT)?;
                out_eof = result.0;
                progress |= result.1;
            }
            if !err_eof {
                let result = read_chunk(&mut stderr, &mut err, MAX_STDERR)?;
                err_eof = result.0;
                progress |= result.1;
            }
            if status.is_none() {
                status = child
                    .0
                    .try_wait()
                    .map_err(|_| failure("Flatpak process status is unavailable"))?;
            }
            if out_eof
                && err_eof
                && let Some(status) = status
            {
                if !status.success() || !err.is_empty() {
                    // Flatpak may skip malformed deployments yet exit zero with stderr
                    // diagnostics. Reject that scope instead of reporting false emptiness.
                    return Err(failure(
                        "Flatpak installation scope could not be read completely",
                    ));
                }
                return parse(&out, scope);
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    pub(super) fn scan(device: DeviceInfo) -> Result<InventorySnapshot, InventoryError> {
        trusted_executable()?;
        snapshot(
            device,
            [capture(InstallScope::User), capture(InstallScope::Machine)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::records::*;
    use super::*;
    use crate::model::{InstallScope, Platform};
    use std::io::Cursor;

    const ROW: &str = "Example\torg.example.App\t1.2\tflathub\torg.example.App/x86_64/stable\n";
    fn device() -> DeviceInfo {
        DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            platform: Platform::Linux,
            architecture: "x86_64".into(),
        }
    }

    #[test]
    fn preserves_observed_metadata_without_publisher_claims() {
        let apps = parse(ROW.as_bytes(), InstallScope::User).unwrap();
        assert_eq!(apps[0].name, "Example");
        assert_eq!(apps[0].native_id.as_deref(), Some("org.example.App"));
        assert_eq!(apps[0].origin.as_deref(), Some("flathub"));
        assert_eq!(apps[0].version.as_deref(), Some("1.2"));
        assert_eq!(apps[0].publisher, None);
        assert_eq!(apps[0].source, crate::model::InventorySource::Flatpak);
        assert_eq!(apps[0].scope, InstallScope::User);
    }

    #[test]
    fn ids_preserve_scope_branch_architecture_and_origin() {
        let base = parse(ROW.as_bytes(), InstallScope::User).unwrap()[0]
            .id
            .clone();
        for row in [
            ROW.replace("x86_64", "aarch64"),
            ROW.replace("stable", "beta"),
            ROW.replace("flathub", "private"),
        ] {
            assert_ne!(
                base,
                parse(row.as_bytes(), InstallScope::User).unwrap()[0].id
            );
        }
        assert_ne!(
            base,
            parse(ROW.as_bytes(), InstallScope::Machine).unwrap()[0].id
        );
        assert_eq!(
            base,
            parse(ROW.replace("1.2", "2.0").as_bytes(), InstallScope::User).unwrap()[0].id
        );
        let variants = format!("{}{}", ROW, ROW.replace("stable", "beta"));
        assert_eq!(
            parse(variants.as_bytes(), InstallScope::User)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn handles_missing_optional_fields_and_exact_duplicates() {
        let row = "\torg.example.App\t\t\torg.example.App/x86_64/stable\n";
        let apps = parse(row.as_bytes(), InstallScope::User).unwrap();
        assert_eq!(apps[0].name, "org.example.App");
        assert_eq!(apps[0].version, None);
        assert_eq!(apps[0].origin, None);
        assert_eq!(
            parse(ROW.repeat(2).as_bytes(), InstallScope::User)
                .unwrap()
                .len(),
            1
        );
        assert!(
            parse(
                format!("{}{}", ROW, ROW.replace("1.2", "2.0")).as_bytes(),
                InstallScope::User
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_or_truncated_tabular_data() {
        for data in [
            "Example\torg.example.App\n".into(),
            ROW.replace("Example", "Bad\rName"),
            ROW.replace("org.example.App/x86_64", "org.other.App/x86_64"),
            ROW.replace("stable", "stable/extra"),
            ROW.replace("Example", &"a".repeat(513)),
            "\n\n".into(),
            "Name\tApplication ID\tVersion\tOrigin\tRef\n".into(),
        ] {
            assert!(parse(data.as_bytes(), InstallScope::User).is_err());
        }
        assert!(parse(&[0xff], InstallScope::User).is_err());
        assert!(parse(b"\n", InstallScope::User).unwrap().is_empty());
        assert!(parse(b"", InstallScope::User).unwrap().is_empty());
        assert!(parse(ROW.as_bytes(), InstallScope::Unknown).is_err());
    }

    #[test]
    fn rejects_byte_and_row_limits() {
        assert_eq!(
            parse(&vec![b' '; MAX_STDOUT + 1], InstallScope::User),
            Err(InventoryError::LimitExceeded)
        );
        assert_eq!(
            parse(ROW.repeat(10_001).as_bytes(), InstallScope::User),
            Err(InventoryError::LimitExceeded)
        );
    }

    #[test]
    fn bounds_stdout_and_stderr_reads() {
        for limit in [MAX_STDOUT, MAX_STDERR] {
            let mut reader = Cursor::new(vec![0; limit + 1]);
            let mut output = Vec::new();
            while read_chunk(&mut reader, &mut output, limit).is_ok() {}
            assert!(output.len() <= limit);
        }
        let mut reader = Cursor::new(b"abc");
        let mut output = Vec::new();
        assert_eq!(
            read_chunk(&mut reader, &mut output, 3).unwrap(),
            (false, true)
        );
        assert_eq!(
            read_chunk(&mut reader, &mut output, 3).unwrap(),
            (true, false)
        );
    }

    #[test]
    fn partial_scope_success_is_explicit_and_complete_failure_is_error() {
        let result = snapshot(
            device(),
            [
                parse(ROW.as_bytes(), InstallScope::User),
                Err(failure("private raw details")),
            ],
        )
        .unwrap();
        assert_eq!(result.apps.len(), 1);
        assert_eq!(result.warnings.len(), 2);
        assert!(result.warnings[1].contains("Default-system"));
        assert!(!result.warnings[1].contains("private raw details"));
        assert!(snapshot(device(), [Err(failure("one")), Err(failure("two"))]).is_err());
        assert!(
            snapshot(device(), [Ok(vec![]), Ok(vec![])])
                .unwrap()
                .apps
                .is_empty()
        );
    }
}
