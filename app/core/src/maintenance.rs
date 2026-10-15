//! Small, separately locked maintenance policy and write-ahead job ledger.
use crate::model::DeviceInfo;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
// Room for the supported 512 policies, checks, update intents and retained interruptions.
pub const MAX_JOBS: usize = 2048;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_secs())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramPolicy {
    pub app_id: String,
    pub winget_id: String,
    pub display_name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MaintenanceAction {
    CheckPrograms,
    UpdateProgram { app_id: String, winget_id: String },
    CheckKitlane,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceRequest {
    pub id: String,
    pub action: MaintenanceAction,
    pub created_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ProgramCheck,
    ProgramUpdate,
    KitlaneCheck,
    KitlaneUpdate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Current,
    Available,
    Succeeded,
    Deferred,
    Failed,
    Unsupported,
    Indeterminate,
    Cancelled,
}

impl JobStatus {
    pub fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Deferred)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceJob {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub app_id: Option<String>,
    pub winget_id: Option<String>,
    pub display_name: String,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub provenance: String,
    pub outcome: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub pid: Option<u32>,
    pub automatic: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceState {
    pub schema: u32,
    pub device: Option<DeviceInfo>,
    pub programs: Vec<ProgramPolicy>,
    pub kitlane_auto_update: bool,
    pub requests: Vec<MaintenanceRequest>,
    pub jobs: Vec<MaintenanceJob>,
    pub next_check_at: u64,
    pub agent_status: String,
    #[serde(default)]
    pub accepted_alternatives: BTreeMap<String, String>,
}

impl Default for MaintenanceState {
    fn default() -> Self {
        Self {
            schema: 1,
            device: None,
            programs: vec![],
            kitlane_auto_update: false,
            requests: vec![],
            jobs: vec![],
            next_check_at: 0,
            agent_status: "Disabled".into(),
            accepted_alternatives: BTreeMap::new(),
        }
    }
}

fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|id| !id.is_nil() && id.to_string() == s)
}
fn identity(app: &str, winget: &str) -> bool {
    text(app, 256)
        && !winget.starts_with(['-', '.'])
        && winget.len() <= 256
        && winget.contains('.')
        && winget
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

impl MaintenanceState {
    pub fn enabled(&self) -> bool {
        self.kitlane_auto_update || self.programs.iter().any(|p| p.enabled)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.programs.len() > 512
            || self.requests.len() > 64
            || self.jobs.len() > MAX_JOBS
            || self.accepted_alternatives.len() > 256
            || !text(&self.agent_status, 1024)
        {
            return Err("Invalid maintenance schema or record limits".into());
        }
        if let Some(device) = &self.device {
            device.validate().map_err(str::to_owned)?;
        }
        let mut keys = HashSet::new();
        for p in &self.programs {
            if !identity(&p.app_id, &p.winget_id)
                || !text(&p.display_name, 512)
                || !keys.insert(&p.app_id)
            {
                return Err("Invalid or duplicate program policy".into());
            }
        }
        let mut ids = HashSet::new();
        for r in &self.requests {
            if !uuid(&r.id) || !ids.insert(&r.id) {
                return Err("Invalid request ID".into());
            }
            if let MaintenanceAction::UpdateProgram { app_id, winget_id } = &r.action
                && !identity(app_id, winget_id)
            {
                return Err("Invalid requested program identity".into());
            }
        }
        ids.clear();
        for j in &self.jobs {
            if !uuid(&j.id)
                || !ids.insert(&j.id)
                || !text(&j.display_name, 512)
                || !text(&j.provenance, 1024)
                || !text(&j.outcome, 2048)
                || j.old_version.as_deref().is_some_and(|s| !text(s, 256))
                || j.new_version.as_deref().is_some_and(|s| !text(s, 256))
            {
                return Err("Invalid job metadata".into());
            }
            match j.kind {
                JobKind::ProgramCheck | JobKind::ProgramUpdate
                    if !j
                        .app_id
                        .as_deref()
                        .zip(j.winget_id.as_deref())
                        .is_some_and(|(a, w)| identity(a, w)) =>
                {
                    return Err("Invalid job identity".into());
                }
                JobKind::KitlaneCheck | JobKind::KitlaneUpdate
                    if j.app_id.is_some() || j.winget_id.is_some() =>
                {
                    return Err("Invalid Kitlane job identity".into());
                }
                _ => {}
            }
        }
        if self
            .accepted_alternatives
            .iter()
            .any(|(k, v)| !text(k, 256) || !text(v, 256))
        {
            return Err("Invalid accepted alternative".into());
        }
        Ok(())
    }
    pub fn enqueue(&mut self, action: MaintenanceAction) -> Result<String, String> {
        if let Some(r) = self.requests.iter().find(|r| r.action == action) {
            return Ok(r.id.clone());
        }
        if self.requests.len() >= 64 {
            return Err("Maintenance request queue is full".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.requests.push(MaintenanceRequest {
            id: id.clone(),
            action,
            created_at: now(),
        });
        Ok(id)
    }
    pub fn add_job(&mut self, job: MaintenanceJob) -> Result<(), String> {
        while self.jobs.len() >= MAX_JOBS {
            let Some(index) = self
                .jobs
                .iter()
                .position(|j| !j.status.active() && j.status != JobStatus::Indeterminate)
            else {
                return Err("Maintenance job queue is full; review interrupted jobs".into());
            };
            self.jobs.remove(index);
        }
        self.jobs.push(job);
        Ok(())
    }
}

pub struct MaintenanceStore {
    directory: PathBuf,
}
impl MaintenanceStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    fn lock(&self) -> Result<File, String> {
        fs::create_dir_all(&self.directory)
            .map_err(|e| format!("Cannot create maintenance directory: {e}"))?;
        let f = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("maintenance.lock"))
            .map_err(|e| format!("Cannot open maintenance lock: {e}"))?;
        // Std File locks are available since Rust 1.89. Dropping File releases the lock.
        f.lock()
            .map_err(|e| format!("Cannot lock maintenance state: {e}"))?;
        Ok(f)
    }
    fn read(&self) -> Result<MaintenanceState, String> {
        let file = match File::open(self.directory.join("maintenance.json")) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MaintenanceState::default());
            }
            Err(e) => return Err(format!("Cannot read maintenance state: {e}")),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Cannot read maintenance state: {e}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Maintenance state exceeds size limit; preserving file".into());
        }
        let state: MaintenanceState = serde_json::from_slice(&bytes)
            .map_err(|_| "Maintenance state is corrupt; preserving file".to_owned())?;
        state.validate()?;
        Ok(state)
    }
    pub fn load(&self) -> Result<MaintenanceState, String> {
        let _lock = self.lock()?;
        self.read()
    }
    pub fn mutate<T>(
        &self,
        mutation: impl FnOnce(&mut MaintenanceState) -> Result<T, String>,
    ) -> Result<T, String> {
        let _lock = self.lock()?;
        let mut state = self.read()?;
        let before = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
        let result = mutation(&mut state)?;
        state.validate()?;
        let bytes = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Maintenance state exceeds size limit".into());
        }
        if bytes == before {
            return Ok(result);
        }
        let temporary = self
            .directory
            .join(format!("maintenance-{}.tmp", uuid::Uuid::new_v4()));
        let write = (|| -> std::io::Result<()> {
            let mut f = File::create_new(&temporary)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&temporary, self.directory.join("maintenance.json"))?;
            #[cfg(unix)]
            File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if let Err(e) = write {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Cannot save maintenance state: {e}"));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    #[test]
    fn unchanged_maintenance_does_not_create_or_rewrite_state() {
        let fixture = Fixture::new();
        fixture.store().mutate(|_| Ok(())).unwrap();
        assert!(!fixture.0.join("maintenance.json").exists());
    }
    impl Fixture {
        fn new() -> Self {
            Self(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../target")
                    .join(format!("kitlane-maintenance-test-{}", uuid::Uuid::new_v4())),
            )
        }
        fn store(&self) -> MaintenanceStore {
            MaintenanceStore::new(self.0.clone())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn disabled_by_default_and_round_trip() {
        let f = Fixture::new();
        let s = f.store();
        assert!(!s.load().unwrap().enabled());
        s.mutate(|s| {
            s.kitlane_auto_update = true;
            s.enqueue(MaintenanceAction::CheckKitlane)?;
            Ok(())
        })
        .unwrap();
        let loaded = s.load().unwrap();
        assert!(loaded.enabled());
        assert_eq!(loaded.requests.len(), 1);
        s.mutate(|s| {
            s.enqueue(MaintenanceAction::CheckKitlane)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(s.load().unwrap().requests.len(), 1);
    }
    #[test]
    fn invalid_mutation_preserves_prior_bytes() {
        let f = Fixture::new();
        let s = f.store();
        s.mutate(|state| {
            state.agent_status = "Ready".into();
            Ok(())
        })
        .unwrap();
        let path = f.0.join("maintenance.json");
        let old = fs::read(&path).unwrap();
        assert!(
            s.mutate(|s| {
                s.schema = 2;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(old, fs::read(path).unwrap());
    }
    #[test]
    fn corrupt_unknown_and_oversized_files_never_overwritten() {
        let f = Fixture::new();
        let s = f.store();
        s.mutate(|_| Ok(())).unwrap();
        let path = f.0.join("maintenance.json");
        for data in [
            b"{broken".to_vec(),
            b"{\"schema\":99}".to_vec(),
            vec![b' '; MAX_BYTES as usize + 1],
        ] {
            fs::write(&path, &data).unwrap();
            assert!(s.load().is_err());
            assert!(s.mutate(|_| Ok(())).is_err());
            assert_eq!(fs::read(&path).unwrap(), data);
        }
    }
    #[test]
    fn queue_and_identity_validation() {
        let mut s = MaintenanceState::default();
        s.enqueue(MaintenanceAction::UpdateProgram {
            app_id: "a".into(),
            winget_id: "--all".into(),
        })
        .unwrap();
        assert!(s.validate().is_err());
        s.requests.clear();
        for _ in 0..65 {
            s.requests.push(MaintenanceRequest {
                id: uuid::Uuid::new_v4().to_string(),
                action: MaintenanceAction::CheckKitlane,
                created_at: now(),
            });
        }
        assert!(s.validate().is_err());
        s.requests.clear();
        s.device = Some(DeviceInfo {
            id: "bad".into(),
            name: "name".into(),
            platform: crate::model::Platform::Windows,
            architecture: "x86_64".into(),
        });
        assert!(s.validate().is_err());
    }
    #[test]
    fn concurrent_mutations_do_not_lose_writes() {
        let f = Fixture::new();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = f.0.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        MaintenanceStore::new(path.clone())
                            .mutate(|s| {
                                s.next_check_at += 1;
                                Ok(())
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(f.store().load().unwrap().next_check_at, 80);
    }
}
