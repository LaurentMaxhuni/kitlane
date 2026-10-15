//! Independent per-user update coordinator. No redb connection or UI lifetime dependency.
use crate::self_update::{self, CheckResult, StagedUpdate};
use kitlane_core::{
    maintenance::{
        self, JobKind, JobStatus, MaintenanceAction, MaintenanceJob, MaintenanceState,
        MaintenanceStore, ProgramPolicy,
    },
    model::DeviceInfo,
    program_updates::{self, ProgramUpdateOutcome, ProgramUpdateRequest, RunningProgramUpdate},
};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

pub fn state_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|path| PathBuf::from(path).join("Library/Application Support"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")));
    let base = base.ok_or("Local application data directory is unavailable")?;
    if !base.is_absolute() {
        return Err("Local application data directory must be absolute".into());
    }
    Ok(base.join("Kitlane"))
}
pub fn load() -> Result<MaintenanceState, String> {
    MaintenanceStore::new(state_dir()?).load()
}
pub fn mutate<T>(f: impl FnOnce(&mut MaintenanceState) -> Result<T, String>) -> Result<T, String> {
    MaintenanceStore::new(state_dir()?).mutate(f)
}
pub fn enqueue(action: MaintenanceAction) -> Result<String, String> {
    let id = mutate(|s| s.enqueue(action))?;
    ensure_running()?;
    Ok(id)
}
fn lock_file(dir: &Path, name: &str) -> Result<File, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(name))
        .map_err(|e| e.to_string())
}
/// Retain this shared lock for the entire UI lifetime, including multiple UI windows.
pub fn hold_ui_lock() -> Result<File, String> {
    let file = lock_file(&state_dir()?, "kitlane-ui.lock")?;
    file.lock_shared()
        .map_err(|e| format!("Cannot lock Kitlane UI lifetime: {e}"))?;
    Ok(file)
}
fn sibling_agent() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe
        .parent()
        .ok_or("Executable has no installation directory")?;
    let agent = parent.join(if cfg!(windows) {
        "kitlane-agent.exe"
    } else {
        "kitlane-agent"
    });
    if !agent.is_file() {
        return Err("Background agent is missing from the Kitlane installation".into());
    }
    Ok(agent)
}
#[cfg(windows)]
fn configure_startup(enabled: bool, agent: Option<&Path>) -> Result<(), String> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
    };
    let root = RegKey::predef(HKEY_CURRENT_USER);
    if !enabled {
        let key = match root.open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_SET_VALUE,
        ) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        return match key.delete_value("KitlaneMaintenance") {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let path = agent
        .ok_or("Background agent path is unavailable")?
        .to_str()
        .ok_or("Background agent path is not valid Unicode")?;
    if path.contains(['"', '\r', '\n']) {
        return Err("Invalid background agent path".into());
    }
    let (key, _) = root
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|e| e.to_string())?;
    key.set_value("KitlaneMaintenance", &format!("\"{path}\" --background"))
        .map_err(|e| e.to_string())
}
#[cfg(not(windows))]
fn configure_startup(enabled: bool, _: Option<&Path>) -> Result<(), String> {
    if enabled {
        Err("Automatic startup is unsupported on this platform".into())
    } else {
        Ok(())
    }
}
pub fn ensure_running() -> Result<(), String> {
    let state = load()?;
    let agent = sibling_agent();
    if let Err(e) = configure_startup(state.enabled(), agent.as_deref().ok()) {
        mutate(|s| {
            s.agent_status = format!("Startup registration unavailable: {e}");
            Ok(())
        })?;
        return Err(e);
    }
    if !state.enabled()
        && state.requests.is_empty()
        && !state.jobs.iter().any(|j| j.status.active())
    {
        return Ok(());
    }
    let agent = agent?;
    let mut command = Command::new(agent);
    command
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("KITLANE_TYPESAFE_API_KEY")
        .env_remove("KITLANE_UPDATE_SIGNING_KEY");
    command.arg("--background");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map_err(|e| format!("Cannot launch background agent: {e}"))?;
    Ok(())
}
fn clean(value: impl AsRef<str>, max: usize) -> String {
    let result: String = value
        .as_ref()
        .chars()
        .filter(|c| !c.is_control())
        .take(max / 4)
        .collect();
    if result.trim().is_empty() {
        "No details supplied".into()
    } else {
        result
    }
}
fn make_job(kind: JobKind, policy: Option<&ProgramPolicy>, automatic: bool) -> MaintenanceJob {
    MaintenanceJob {
        id: kitlane_core::new_device_id(),
        kind,
        status: JobStatus::Queued,
        app_id: policy.map(|p| p.app_id.clone()),
        winget_id: policy.map(|p| p.winget_id.clone()),
        display_name: policy.map_or_else(|| "Kitlane".into(), |p| p.display_name.clone()),
        old_version: None,
        new_version: None,
        provenance: match kind {
            JobKind::ProgramCheck | JobKind::ProgramUpdate => {
                "Exact WinGet ID; validated official winget source"
            }
            _ => "Signed Kitlane release manifest; pinned release trust root",
        }
        .into(),
        outcome: "Queued".into(),
        created_at: maintenance::now(),
        updated_at: maintenance::now(),
        pid: None,
        automatic,
    }
}
fn permitted(s: &MaintenanceState, j: &MaintenanceJob) -> bool {
    match j.kind {
        JobKind::ProgramUpdate => s.programs.iter().any(|p| {
            p.enabled
                && j.app_id.as_deref() == Some(p.app_id.as_str())
                && j.winget_id.as_deref() == Some(p.winget_id.as_str())
                && j.display_name == p.display_name
        }),
        JobKind::KitlaneUpdate => s.kitlane_auto_update,
        _ => {
            !j.automatic
                || match j.kind {
                    JobKind::ProgramCheck => s.programs.iter().any(|p| {
                        p.enabled
                            && j.app_id.as_deref() == Some(p.app_id.as_str())
                            && j.winget_id.as_deref() == Some(p.winget_id.as_str())
                    }),
                    _ => s.kitlane_auto_update,
                }
        }
    }
}
fn add_unique(s: &mut MaintenanceState, job: MaintenanceJob) -> Result<(), String> {
    if !s.jobs.iter().any(|j| {
        j.status.active()
            && j.kind == job.kind
            && j.app_id == job.app_id
            && j.winget_id == job.winget_id
    }) {
        s.add_job(job)?;
    }
    Ok(())
}
fn uncertain(s: &MaintenanceState, j: &MaintenanceJob) -> bool {
    s.jobs.iter().any(|prior| {
        prior.status == JobStatus::Indeterminate
            && prior.kind == j.kind
            && prior.app_id == j.app_id
            && prior.winget_id == j.winget_id
    })
}
fn schedule(s: &mut MaintenanceState) -> Result<(), String> {
    while let Some(request) = s.requests.first().cloned() {
        match request.action {
            MaintenanceAction::CheckPrograms => {
                for p in s.programs.clone() {
                    add_unique(s, make_job(JobKind::ProgramCheck, Some(&p), false))?;
                }
            }
            MaintenanceAction::UpdateProgram { app_id, winget_id } => {
                // Read-only discovery saves exact old/target versions before any installer intent.
                if let Some(p) = s
                    .programs
                    .iter()
                    .find(|p| p.enabled && p.app_id == app_id && p.winget_id == winget_id)
                    .cloned()
                {
                    add_unique(s, make_job(JobKind::ProgramCheck, Some(&p), false))?;
                } else {
                    s.agent_status =
                        "Requested program update requires an exact enabled program policy".into();
                }
            }
            MaintenanceAction::CheckKitlane => {
                add_unique(s, make_job(JobKind::KitlaneCheck, None, false))?
            }
        }
        s.requests.remove(0);
    }
    let now = maintenance::now();
    if s.enabled() && now >= s.next_check_at {
        for p in s.programs.clone().into_iter().filter(|p| p.enabled) {
            add_unique(s, make_job(JobKind::ProgramCheck, Some(&p), true))?;
        }
        if s.kitlane_auto_update {
            add_unique(s, make_job(JobKind::KitlaneCheck, None, true))?;
        }
        let jitter = std::process::id() as u64 % 901;
        s.next_check_at = now.saturating_add(6 * 60 * 60 + jitter);
    }
    Ok(())
}
fn set_job(
    store: &MaintenanceStore,
    id: &str,
    status: JobStatus,
    outcome: impl AsRef<str>,
) -> Result<(), String> {
    store.mutate(|s| {
        let j = s
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("Maintenance job disappeared")?;
        j.status = status;
        j.outcome = clean(outcome, 2048);
        j.updated_at = maintenance::now();
        Ok(())
    })
}
fn request(j: &MaintenanceJob) -> Result<ProgramUpdateRequest, String> {
    Ok(ProgramUpdateRequest {
        app_id: j.app_id.clone().ok_or("Missing program app ID")?,
        winget_id: j.winget_id.clone().ok_or("Missing program package ID")?,
    })
}
fn program_finished(
    store: &MaintenanceStore,
    id: &str,
    result: ProgramUpdateOutcome,
) -> Result<(), String> {
    store.mutate(|s| {
        let index = s
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or("Maintenance job disappeared")?;
        let j = &mut s.jobs[index];
        let mut updated_identity = None;
        match result {
            ProgramUpdateOutcome::Available {
                old_version,
                new_version,
            } => {
                j.status = JobStatus::Available;
                j.old_version = Some(clean(old_version, 256));
                j.new_version = Some(clean(new_version, 256));
                j.outcome = "Exact package update available".into();
            }
            ProgramUpdateOutcome::Updated {
                old_version,
                new_version,
                app_id,
                display_name,
            } => {
                updated_identity = Some((app_id, display_name));
                j.status = JobStatus::Succeeded;
                j.old_version = Some(clean(old_version, 256));
                j.new_version = Some(clean(new_version, 256));
                j.outcome = "Upgrade finished and inventory verified".into();
            }
            ProgramUpdateOutcome::NoUpdate { version } => {
                j.status = JobStatus::Current;
                j.old_version = Some(clean(version, 256));
                j.outcome = "No verified update available".into();
            }
            ProgramUpdateOutcome::NeedsUserAction { reason, detail } => {
                j.status = if j.kind == JobKind::ProgramUpdate && j.pid.is_some() {
                    JobStatus::Indeterminate
                } else {
                    JobStatus::Failed
                };
                j.outcome = clean(format!("{reason:?}: {detail}"), 2048);
            }
            ProgramUpdateOutcome::Unsupported => {
                j.status = JobStatus::Unsupported;
                j.outcome = "Program updates are unsupported on this platform".into();
            }
        }
        j.updated_at = maintenance::now();
        let completed = j.clone();
        if let Some((app_id, display_name)) = updated_identity {
            for policy in &mut s.programs {
                if completed.app_id.as_deref() == Some(policy.app_id.as_str())
                    && completed.winget_id.as_deref() == Some(policy.winget_id.as_str())
                {
                    policy.app_id = app_id.clone();
                    policy.display_name = display_name.clone();
                }
            }
        }
        if completed.kind == JobKind::ProgramCheck
            && completed.status == JobStatus::Available
            && let Some(p) = s
                .programs
                .iter()
                .find(|p| {
                    p.enabled
                        && completed.app_id.as_deref() == Some(p.app_id.as_str())
                        && completed.winget_id.as_deref() == Some(p.winget_id.as_str())
                        && completed.display_name == p.display_name
                })
                .cloned()
        {
            let mut update = make_job(JobKind::ProgramUpdate, Some(&p), true);
            update.old_version = completed.old_version;
            update.new_version = completed.new_version;
            if !uncertain(s, &update) {
                add_unique(s, update)?;
            }
        }
        Ok(())
    })
}
enum WorkerResult {
    ProgramChecked(ProgramUpdateOutcome),
    ProgramStarted(Result<RunningProgramUpdate, ProgramUpdateOutcome>),
    ProgramPolled(RunningProgramUpdate, Option<ProgramUpdateOutcome>),
    KitlaneChecked(Result<CheckResult, String>),
    KitlaneStaged(Result<StagedUpdate, String>),
}
struct Worker {
    id: String,
    receiver: Receiver<WorkerResult>,
}
fn launch_worker(dir: PathBuf, job: MaintenanceJob, device: Option<DeviceInfo>) -> Worker {
    let (sender, receiver) = mpsc::channel();
    let id = job.id.clone();
    thread::spawn(move || {
        let result = match job.kind {
            JobKind::ProgramCheck | JobKind::ProgramUpdate => {
                let failure = |detail: String| ProgramUpdateOutcome::NeedsUserAction {
                    reason: program_updates::ProgramUpdateReason::InvalidRequest,
                    detail,
                };
                match device.zip(request(&job).ok()) {
                    Some((device, request)) => {
                        if job.kind == JobKind::ProgramCheck {
                            WorkerResult::ProgramChecked(program_updates::check_program_update(
                                device, &request,
                            ))
                        } else {
                            WorkerResult::ProgramStarted(match job.old_version.as_deref().zip(job.new_version.as_deref()) {
                            Some((old,new))=>program_updates::start_program_update_exact(device,&request,old,new),
                            None=>Err(failure("Exact old and target versions must be durably recorded before installation".into())),
                        })
                        }
                    }
                    None => WorkerResult::ProgramChecked(failure(
                        "Validated device identity is required".into(),
                    )),
                }
            }
            JobKind::KitlaneCheck => WorkerResult::KitlaneChecked(self_update::check(&dir)),
            JobKind::KitlaneUpdate => WorkerResult::KitlaneStaged(match self_update::check(&dir) {
                Ok(CheckResult::Available(update)) => self_update::stage(&update, &dir),
                Ok(_) => Err("Kitlane update is no longer available or configured".into()),
                Err(e) => Err(e),
            }),
        };
        let _ = sender.send(result);
    });
    Worker { id, receiver }
}
/// Interrupted installers are never replayed. Inventory observations aid manual review only.
fn reconcile(store: &MaintenanceStore) -> Result<(), String> {
    let state = store.load()?;
    let interrupted: Vec<_> = state
        .jobs
        .iter()
        .filter(|j| j.status == JobStatus::Running)
        .cloned()
        .collect();
    let observed = if interrupted.iter().any(|j| j.kind == JobKind::ProgramUpdate) {
        state
            .device
            .clone()
            .and_then(|d| kitlane_core::inventory::scan(d).ok())
    } else {
        None
    };
    store.mutate(|s|{
        for old in interrupted {
            let Some(j)=s.jobs.iter_mut().find(|j|j.id==old.id)else{continue};
            if matches!(j.kind,JobKind::ProgramCheck|JobKind::KitlaneCheck) {j.status=JobStatus::Queued;j.outcome="Restarted interrupted read-only check".into();}
            else if j.kind==JobKind::KitlaneUpdate && j.new_version.as_deref()==Some(env!("CARGO_PKG_VERSION")) {j.status=JobStatus::Succeeded;j.outcome="Installed Kitlane version matches the signed update target after restart".into();}
            else {j.status=JobStatus::Indeterminate;j.outcome=clean(if let Some(app)=observed.as_ref().and_then(|snapshot|snapshot.apps.iter().find(|a|Some(&a.id)==j.app_id.as_ref())) {format!("Interrupted update; observed version {}. Review installer state before requesting another update.",app.version.as_deref().unwrap_or("unknown"))} else {"Interrupted update; completion is unknown. Review installer state before requesting another update.".into()},2048);}
            j.updated_at=maintenance::now();
        }
        Ok(())
    })
}
pub fn run() -> Result<(), String> {
    let dir = state_dir()?;
    let daemon = lock_file(&dir, "kitlane-agent.lock")?;
    match daemon.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(e) => return Err(format!("Cannot acquire agent lock: {e}")),
    }
    let store = MaintenanceStore::new(dir.clone());
    reconcile(&store)?;
    let mut worker: Option<Worker> = None;
    let mut installer: Option<(String, RunningProgramUpdate)> = None;
    let mut staged: Option<(String, StagedUpdate)> = None;
    let mut previous_enabled = None;
    loop {
        let state = store.load()?;
        if previous_enabled != Some(state.enabled()) {
            let startup = configure_startup(state.enabled(), sibling_agent().ok().as_deref());
            store.mutate(|s| {
                s.agent_status = match startup {
                    Ok(()) => "Background agent ready".into(),
                    Err(e) => clean(format!("Startup unavailable: {e}"), 1024),
                };
                Ok(())
            })?;
            previous_enabled = Some(state.enabled());
        }
        if let Some(active) = worker.as_ref() {
            match active.receiver.try_recv() {
                Ok(result) => {
                    let id = active.id.clone();
                    worker = None;
                    match result {
                        WorkerResult::ProgramChecked(outcome) => {
                            program_finished(&store, &id, outcome)?
                        }
                        WorkerResult::ProgramStarted(Ok(running)) => {
                            store.mutate(|s| {
                                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                                    j.pid = Some(running.pid());
                                    j.outcome =
                                        "Installer running; policy changes apply to future jobs"
                                            .into();
                                }
                                Ok(())
                            })?;
                            installer = Some((id, running));
                        }
                        WorkerResult::ProgramStarted(Err(outcome)) => {
                            program_finished(&store, &id, outcome)?
                        }
                        WorkerResult::ProgramPolled(running, Some(outcome)) => {
                            drop(running);
                            program_finished(&store, &id, outcome)?;
                        }
                        WorkerResult::ProgramPolled(running, None) => {
                            installer = Some((id, running));
                        }
                        WorkerResult::KitlaneChecked(result) => store.mutate(|s| {
                            let j = s
                                .jobs
                                .iter_mut()
                                .find(|j| j.id == id)
                                .ok_or("Maintenance job disappeared")?;
                            j.old_version = Some(env!("CARGO_PKG_VERSION").into());
                            j.updated_at = maintenance::now();
                            match result {
                                Ok(CheckResult::Available(update)) => {
                                    j.status = JobStatus::Available;
                                    j.new_version = Some(update.version().into());
                                    j.outcome = "Signed release update available".into();
                                }
                                Ok(CheckResult::Current) => {
                                    j.status = JobStatus::Current;
                                    j.outcome = "Kitlane is current".into();
                                }
                                Ok(CheckResult::NotConfigured(reason))
                                | Ok(CheckResult::Unsupported(reason)) => {
                                    j.status = JobStatus::Unsupported;
                                    j.outcome = clean(reason, 2048);
                                }
                                Err(e) => {
                                    j.status = JobStatus::Failed;
                                    j.outcome = clean(e, 2048);
                                }
                            }
                            let completed = j.clone();
                            if completed.status == JobStatus::Available && s.kitlane_auto_update {
                                let mut update = make_job(JobKind::KitlaneUpdate, None, true);
                                update.old_version = completed.old_version;
                                update.new_version = completed.new_version;
                                if !uncertain(s, &update) {
                                    add_unique(s, update)?;
                                }
                            }
                            Ok(())
                        })?,
                        WorkerResult::KitlaneStaged(Ok(update)) => {
                            store.mutate(|s|{let j=s.jobs.iter_mut().find(|j|j.id==id).ok_or("Maintenance job disappeared")?;j.new_version=Some(update.version().into());j.status=JobStatus::Deferred;j.updated_at=maintenance::now();j.outcome="Verified installer staged; waiting for Kitlane windows to close".into();Ok(())})?;
                            staged = Some((id, update));
                        }
                        WorkerResult::KitlaneStaged(Err(e)) => {
                            set_job(&store, &id, JobStatus::Failed, e)?
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    let id = active.id.clone();
                    worker = None;
                    set_job(
                        &store,
                        &id,
                        JobStatus::Indeterminate,
                        "Maintenance worker ended unexpectedly; review before retry",
                    )?;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if worker.is_none()
            && let Some((id, mut running)) = installer.take()
        {
            // Exit verification can scan inventory; keep it off the coordinator loop too.
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let outcome = running.poll();
                let _ = sender.send(WorkerResult::ProgramPolled(running, outcome));
            });
            worker = Some(Worker { id, receiver });
        }
        if let Some((id, update)) = staged.as_ref() {
            let current = store.load()?;
            if !current.kitlane_auto_update {
                let id = id.clone();
                staged = None;
                set_job(
                    &store,
                    &id,
                    JobStatus::Cancelled,
                    "Kitlane automatic updates disabled before installation",
                )?;
            } else {
                let ui = lock_file(&dir, "kitlane-ui.lock")?;
                match ui.try_lock() {
                    Ok(()) => {
                        let id = id.clone();
                        // Persist intent before invoking the installer. Exit releases our executable.
                        let permitted = store.mutate(|s| {
                            if !s.kitlane_auto_update {
                                return Ok(false);
                            }
                            let j = s
                                .jobs
                                .iter_mut()
                                .find(|j| j.id == id)
                                .ok_or("Maintenance job disappeared")?;
                            j.status = JobStatus::Running;
                            j.outcome = "Launching verified Kitlane installer".into();
                            j.updated_at = maintenance::now();
                            Ok(true)
                        })?;
                        if permitted {
                            match self_update::install(update) {
                                Ok(pid) => {
                                    store.mutate(|s| {
                                        if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                                            j.pid = Some(pid);
                                            j.outcome =
                                                "Installer launched; agent exiting for replacement"
                                                    .into();
                                        }
                                        Ok(())
                                    })?;
                                    return Ok(());
                                }
                                Err(e) => {
                                    staged = None;
                                    set_job(&store, &id, JobStatus::Failed, e)?;
                                }
                            }
                        }
                    }
                    Err(std::fs::TryLockError::WouldBlock) => {}
                    Err(e) => return Err(format!("Cannot determine UI lifetime: {e}")),
                }
            }
        }
        if worker.is_none() && installer.is_none() && staged.is_none() {
            let next=store.mutate(|s|{
                schedule(s)?;
                let Some(index)=s.jobs.iter().position(|j|matches!(j.status,JobStatus::Queued|JobStatus::Deferred))else{return Ok(None)};
                if !permitted(s,&s.jobs[index]) || (matches!(s.jobs[index].kind,JobKind::ProgramUpdate|JobKind::KitlaneUpdate) && uncertain(s,&s.jobs[index])) {let j=&mut s.jobs[index];j.status=JobStatus::Cancelled;j.updated_at=maintenance::now();j.outcome="Policy disabled, identity changed, or interrupted installer requires manual review".into();return Ok(None);}
                let j=&mut s.jobs[index];j.status=JobStatus::Running;j.updated_at=maintenance::now();j.outcome="Worker started; intent saved before external operation".into();
                Ok(Some((j.clone(),s.device.clone())))
            })?;
            if let Some((job, device)) = next {
                worker = Some(launch_worker(dir.clone(), job, device));
            } else {
                let current = store.load()?;
                if !current.enabled()
                    && current.requests.is_empty()
                    && !current.jobs.iter().any(|j| j.status.active())
                {
                    store.mutate(|s| {
                        s.agent_status = "Disabled; background agent stopped".into();
                        Ok(())
                    })?;
                    return Ok(());
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> ProgramPolicy {
        ProgramPolicy {
            app_id: "observed-app".into(),
            winget_id: "Publisher.App".into(),
            display_name: "App".into(),
            enabled: true,
        }
    }
    #[test]
    fn opt_in_requires_all_three_identity_fields() {
        let p = policy();
        let mut s = MaintenanceState::default();
        let mut j = make_job(JobKind::ProgramUpdate, Some(&p), true);
        assert!(!permitted(&s, &j));
        s.programs.push(p);
        assert!(permitted(&s, &j));
        j.display_name = "Other".into();
        assert!(!permitted(&s, &j));
        j.display_name = "App".into();
        s.programs[0].winget_id = "Publisher.Other".into();
        assert!(!permitted(&s, &j));
        assert!(!permitted(
            &s,
            &make_job(JobKind::KitlaneUpdate, None, true)
        ));
    }
    #[test]
    fn interrupted_updates_block_future_installers() {
        let mut s = MaintenanceState::default();
        let mut old = make_job(JobKind::ProgramUpdate, Some(&policy()), true);
        old.status = JobStatus::Indeterminate;
        s.jobs.push(old);
        assert!(uncertain(
            &s,
            &make_job(JobKind::ProgramUpdate, Some(&policy()), true)
        ));
        assert!(!uncertain(
            &s,
            &make_job(JobKind::ProgramCheck, Some(&policy()), true)
        ));
    }
    #[test]
    fn successful_registration_change_preserves_opt_in_and_job_history() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "maintenance-rebind-{}",
                kitlane_core::new_device_id()
            ));
        let store = MaintenanceStore::new(directory.clone());
        let policy = policy();
        let mut job = make_job(JobKind::ProgramUpdate, Some(&policy), true);
        job.status = JobStatus::Running;
        let id = job.id.clone();
        store
            .mutate(|state| {
                state.programs.push(policy.clone());
                state.jobs.push(job);
                Ok(())
            })
            .unwrap();
        program_finished(
            &store,
            &id,
            ProgramUpdateOutcome::Updated {
                old_version: "1.0".into(),
                new_version: "1.1".into(),
                app_id: "new-registration".into(),
                display_name: "App 1.1".into(),
            },
        )
        .unwrap();
        let state = store.load().unwrap();
        assert!(state.programs[0].enabled);
        assert_eq!(state.programs[0].app_id, "new-registration");
        assert_eq!(state.programs[0].display_name, "App 1.1");
        assert_eq!(
            state.jobs[0].app_id.as_deref(),
            Some(policy.app_id.as_str())
        );
        assert_eq!(state.jobs[0].status, JobStatus::Succeeded);
        // Unique test directory, always confined to this workspace's ignored target.
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn installer_failure_requires_review_before_another_attempt() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "maintenance-failed-{}",
                kitlane_core::new_device_id()
            ));
        let store = MaintenanceStore::new(directory.clone());
        let policy = policy();
        let mut job = make_job(JobKind::ProgramUpdate, Some(&policy), true);
        job.status = JobStatus::Running;
        job.pid = Some(123); // Fixture only: no process is started.
        let id = job.id.clone();
        store
            .mutate(|state| {
                state.programs.push(policy.clone());
                state.jobs.push(job);
                Ok(())
            })
            .unwrap();
        program_finished(
            &store,
            &id,
            ProgramUpdateOutcome::NeedsUserAction {
                reason: program_updates::ProgramUpdateReason::VerificationFailed,
                detail: "Fixture lost installer state".into(),
            },
        )
        .unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.jobs[0].status, JobStatus::Indeterminate);
        assert!(uncertain(
            &state,
            &make_job(JobKind::ProgramUpdate, Some(&policy), true)
        ));
        let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .canonicalize()
            .unwrap();
        assert!(directory.canonicalize().unwrap().starts_with(expected));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn full_supported_program_policy_fits_the_durable_queue() {
        let mut state = MaintenanceState::default();
        for index in 0..512 {
            state.programs.push(ProgramPolicy {
                app_id: format!("app-{index}"),
                winget_id: format!("Publisher.App{index}"),
                display_name: format!("App {index}"),
                enabled: true,
            });
        }
        state.kitlane_auto_update = true;
        schedule(&mut state).unwrap();
        assert_eq!(state.jobs.len(), 513);
    }

    #[test]
    fn scheduled_work_is_bounded_opted_in_and_deduplicated() {
        let mut s = MaintenanceState::default();
        schedule(&mut s).unwrap();
        assert!(s.jobs.is_empty());
        s.programs.push(policy());
        s.kitlane_auto_update = true;
        schedule(&mut s).unwrap();
        assert_eq!(s.jobs.len(), 2);
        s.next_check_at = 0;
        schedule(&mut s).unwrap();
        assert_eq!(s.jobs.len(), 2);
        assert!(s.next_check_at > maintenance::now() + 6 * 60 * 60 - 1);
    }
}
