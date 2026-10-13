//! Local durable state. Schema 1 stores JSON records separately from device identity.
use crate::model::{AppSettings, DeviceInfo, InventorySnapshot};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fmt,
    fs::OpenOptions,
    path::{Path, PathBuf},
};

const META: TableDefinition<&str, u64> = TableDefinition::new("kitlane_meta");
const RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("kitlane_records");
const SCHEMA_VERSION: u64 = 1;
const DEVICE_LIMIT: usize = 4096;
const SNAPSHOT_LIMIT: usize = 32 * 1024 * 1024;
const SETTINGS_LIMIT: usize = 1024;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Database(redb::Error),
    Json(serde_json::Error),
    UnsupportedSchema(u64),
    InvalidData(&'static str),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Local database I/O failed: {e}"),
            Self::Database(e) => write!(f, "Local database operation failed: {e}"),
            Self::Json(e) => write!(f, "Invalid local database record: {e}"),
            Self::UnsupportedSchema(v) => write!(f, "Unsupported local database schema {v}"),
            Self::InvalidData(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Database(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

fn database_error(e: impl Into<redb::Error>) -> StoreError {
    StoreError::Database(e.into())
}

fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, StoreError> {
    if bytes.len() > limit {
        return Err(StoreError::InvalidData(
            "Local database record exceeds size limit",
        ));
    }
    serde_json::from_slice(bytes).map_err(StoreError::Json)
}

fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, StoreError> {
    let bytes = serde_json::to_vec(value).map_err(StoreError::Json)?;
    if bytes.len() > limit {
        return Err(StoreError::InvalidData(
            "Local database record exceeds size limit",
        ));
    }
    Ok(bytes)
}

fn read_record<T: DeserializeOwned>(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
    key: &str,
    limit: usize,
) -> Result<Option<T>, StoreError> {
    table
        .get(key)
        .map_err(database_error)?
        .map(|v| decode(v.value(), limit))
        .transpose()
}

fn same_device(left: &DeviceInfo, right: &DeviceInfo) -> bool {
    uuid::Uuid::parse_str(&left.id).ok() == uuid::Uuid::parse_str(&right.id).ok()
}

// Own only the uniquely reserved staging file, never the final database path.
struct InitializingFile(PathBuf);

impl InitializingFile {
    fn reserve(path: &Path) -> Result<Self, StoreError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = parent.join(format!(".kitlane-init-{}.redb", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&temporary).map_err(StoreError::Io)?;
        Ok(Self(temporary))
    }
}

impl Drop for InitializingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn initialize_database(path: &Path) -> Result<(), StoreError> {
    let temporary = InitializingFile::reserve(path)?;
    {
        let db = Database::create(&temporary.0).map_err(database_error)?;
        let txn = db.begin_write().map_err(database_error)?;
        txn.open_table(META)
            .map_err(database_error)?
            .insert("schema_version", SCHEMA_VERSION)
            .map_err(database_error)?;
        txn.open_table(RECORDS).map_err(database_error)?;
        txn.commit().map_err(database_error)?;
    }
    OpenOptions::new()
        .write(true)
        .open(&temporary.0)
        .map_err(StoreError::Io)?
        .sync_all()
        .map_err(StoreError::Io)?;
    // hard_link publishes a fully initialized file exclusively; it cannot replace a winner.
    match std::fs::hard_link(&temporary.0, path) {
        Ok(()) => {
            #[cfg(unix)]
            {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                std::fs::File::open(parent)
                    .map_err(StoreError::Io)?
                    .sync_all()
                    .map_err(StoreError::Io)?;
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(StoreError::Io(e)),
    }
}
pub struct Store {
    db: Database,
}

impl Store {
    /// Creates a database only when the path is absent. Existing files are never reset.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if !path.try_exists().map_err(StoreError::Io)? {
            initialize_database(path)?;
        }
        let db = Database::open(path).map_err(database_error)?;
        {
            let txn = db.begin_read().map_err(database_error)?;
            let meta = txn.open_table(META).map_err(database_error)?;
            let version = meta
                .get("schema_version")
                .map_err(database_error)?
                .ok_or(StoreError::InvalidData(
                    "Local database schema version is missing",
                ))?
                .value();
            if version != SCHEMA_VERSION {
                return Err(StoreError::UnsupportedSchema(version));
            }
            txn.open_table(RECORDS).map_err(database_error)?;
        }
        let store = Self { db };
        store.load_device()?;
        store.load_snapshot()?;
        store.load_settings()?;
        Ok(store)
    }

    pub fn load_device(&self) -> Result<Option<DeviceInfo>, StoreError> {
        let txn = self.db.begin_read().map_err(database_error)?;
        let table = txn.open_table(RECORDS).map_err(database_error)?;
        let device: Option<DeviceInfo> = read_record(&table, "device", DEVICE_LIMIT)?;
        if let Some(device) = &device {
            device.validate().map_err(StoreError::InvalidData)?;
        }
        Ok(device)
    }

    pub fn save_device(&self, device: &DeviceInfo) -> Result<(), StoreError> {
        device.validate().map_err(StoreError::InvalidData)?;
        let bytes = encode(device, DEVICE_LIMIT)?;
        let txn = self.db.begin_write().map_err(database_error)?;
        {
            let mut table = txn.open_table(RECORDS).map_err(database_error)?;
            if let Some(existing) = read_record::<DeviceInfo>(&table, "device", DEVICE_LIMIT)? {
                existing.validate().map_err(StoreError::InvalidData)?;
                if !same_device(&existing, device) {
                    return Err(StoreError::InvalidData(
                        "Saved device identity cannot be replaced",
                    ));
                }
            }
            if let Some(snapshot) =
                read_record::<InventorySnapshot>(&table, "snapshot", SNAPSHOT_LIMIT)?
            {
                snapshot.validate().map_err(StoreError::InvalidData)?;
                if !same_device(&snapshot.device, device) {
                    return Err(StoreError::InvalidData(
                        "Inventory conflicts with saved device identity",
                    ));
                }
            }
            table
                .insert("device", bytes.as_slice())
                .map_err(database_error)?;
        }
        txn.commit().map_err(database_error)
    }

    pub fn load_snapshot(&self) -> Result<Option<InventorySnapshot>, StoreError> {
        let txn = self.db.begin_read().map_err(database_error)?;
        let table = txn.open_table(RECORDS).map_err(database_error)?;
        let snapshot: Option<InventorySnapshot> = read_record(&table, "snapshot", SNAPSHOT_LIMIT)?;
        if let Some(snapshot) = &snapshot {
            snapshot.validate().map_err(StoreError::InvalidData)?;
            let device: DeviceInfo = read_record(&table, "device", DEVICE_LIMIT)?.ok_or(
                StoreError::InvalidData("Inventory has no saved device identity"),
            )?;
            device.validate().map_err(StoreError::InvalidData)?;
            if !same_device(&device, &snapshot.device) {
                return Err(StoreError::InvalidData(
                    "Inventory conflicts with saved device identity",
                ));
            }
        }
        Ok(snapshot)
    }

    /// Device identity must be saved first. The check and snapshot replacement are atomic.
    pub fn save_snapshot(&self, snapshot: &InventorySnapshot) -> Result<(), StoreError> {
        snapshot.validate().map_err(StoreError::InvalidData)?;
        let bytes = encode(snapshot, SNAPSHOT_LIMIT)?;
        let txn = self.db.begin_write().map_err(database_error)?;
        {
            let mut table = txn.open_table(RECORDS).map_err(database_error)?;
            let device: DeviceInfo = read_record(&table, "device", DEVICE_LIMIT)?.ok_or(
                StoreError::InvalidData("Save device identity before inventory"),
            )?;
            device.validate().map_err(StoreError::InvalidData)?;
            if !same_device(&device, &snapshot.device) {
                return Err(StoreError::InvalidData(
                    "Inventory conflicts with saved device identity",
                ));
            }
            if let Some(existing) =
                read_record::<InventorySnapshot>(&table, "snapshot", SNAPSHOT_LIMIT)?
            {
                existing.validate().map_err(StoreError::InvalidData)?;
                if !same_device(&device, &existing.device) {
                    return Err(StoreError::InvalidData(
                        "Inventory conflicts with saved device identity",
                    ));
                }
            }
            table
                .insert("snapshot", bytes.as_slice())
                .map_err(database_error)?;
        }
        txn.commit().map_err(database_error)
    }

    pub fn load_settings(&self) -> Result<AppSettings, StoreError> {
        let txn = self.db.begin_read().map_err(database_error)?;
        let table = txn.open_table(RECORDS).map_err(database_error)?;
        let settings: AppSettings =
            read_record(&table, "settings", SETTINGS_LIMIT)?.unwrap_or_default();
        settings.validate().map_err(StoreError::InvalidData)?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), StoreError> {
        settings.validate().map_err(StoreError::InvalidData)?;
        let bytes = encode(settings, SETTINGS_LIMIT)?;
        let txn = self.db.begin_write().map_err(database_error)?;
        {
            let mut table = txn.open_table(RECORDS).map_err(database_error)?;
            read_record::<AppSettings>(&table, "settings", SETTINGS_LIMIT)?;
            table
                .insert("settings", bytes.as_slice())
                .map_err(database_error)?;
        }
        txn.commit().map_err(database_error)
    }

    /// Commit setup choices together, retaining identity and existing inventory.
    pub fn save_setup(
        &self,
        device: &DeviceInfo,
        settings: &AppSettings,
    ) -> Result<(), StoreError> {
        device.validate().map_err(StoreError::InvalidData)?;
        settings.validate().map_err(StoreError::InvalidData)?;
        let device_bytes = encode(device, DEVICE_LIMIT)?;
        let settings_bytes = encode(settings, SETTINGS_LIMIT)?;
        let txn = self.db.begin_write().map_err(database_error)?;
        {
            let mut table = txn.open_table(RECORDS).map_err(database_error)?;
            let existing: DeviceInfo = read_record(&table, "device", DEVICE_LIMIT)?.ok_or(
                StoreError::InvalidData("Setup has no saved device identity"),
            )?;
            existing.validate().map_err(StoreError::InvalidData)?;
            if existing.id != device.id
                || existing.platform != device.platform
                || existing.architecture != device.architecture
            {
                return Err(StoreError::InvalidData(
                    "Setup cannot replace device identity",
                ));
            }
            read_record::<AppSettings>(&table, "settings", SETTINGS_LIMIT)?;
            if let Some(mut snapshot) =
                read_record::<InventorySnapshot>(&table, "snapshot", SNAPSHOT_LIMIT)?
            {
                snapshot.validate().map_err(StoreError::InvalidData)?;
                if !same_device(device, &snapshot.device) {
                    return Err(StoreError::InvalidData(
                        "Inventory conflicts with saved device identity",
                    ));
                }
                snapshot.device = device.clone();
                let bytes = encode(&snapshot, SNAPSHOT_LIMIT)?;
                table
                    .insert("snapshot", bytes.as_slice())
                    .map_err(database_error)?;
            }
            table
                .insert("device", device_bytes.as_slice())
                .map_err(database_error)?;
            table
                .insert("settings", settings_bytes.as_slice())
                .map_err(database_error)?;
        }
        txn.commit().map_err(database_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_saves_atomically_and_rejects_identity_replacement() {
        let path = TestPath::new();
        let store = Store::open(&path.0).unwrap();
        let original = snapshot(device());
        store.save_device(&original.device).unwrap();
        store.save_snapshot(&original).unwrap();
        let mut renamed = original.device.clone();
        renamed.name = "Studio PC".into();
        let settings = AppSettings {
            onboarding_completed: true,
            scan_on_startup: false,
            ..Default::default()
        };
        store.save_setup(&renamed, &settings).unwrap();
        let mut invalid = renamed.clone();
        invalid.name = "bad\nname".into();
        assert!(store.save_setup(&invalid, &AppSettings::default()).is_err());
        assert!(
            store
                .save_setup(
                    &renamed,
                    &AppSettings {
                        onboarding_step: 4,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert!(
            store
                .save_setup(&device(), &AppSettings::default())
                .is_err()
        );
        drop(store);
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_device().unwrap(), Some(renamed.clone()));
        assert_eq!(store.load_settings().unwrap(), settings);
        assert_eq!(store.load_snapshot().unwrap().unwrap().device, renamed);
    }

    #[test]
    fn unfinished_setup_resumes_with_saved_choices() {
        let path = TestPath::new();
        let store = Store::open(&path.0).unwrap();
        let device = device();
        store.save_device(&device).unwrap();
        let settings = AppSettings {
            dark_theme: true,
            onboarding_step: 2,
            scan_on_startup: false,
            ..Default::default()
        };
        store.save_setup(&device, &settings).unwrap();
        drop(store);
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_settings().unwrap(), settings);
    }
    use crate::model::Platform;

    struct TestPath(std::path::PathBuf);
    impl TestPath {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("kitlane-store-{}.redb", uuid::Uuid::new_v4())))
        }
    }
    impl Drop for TestPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn device() -> DeviceInfo {
        DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test device".into(),
            platform: Platform::Windows,
            architecture: "x86_64".into(),
        }
    }
    fn snapshot(device: DeviceInfo) -> InventorySnapshot {
        InventorySnapshot {
            device,
            scanned_at_unix_secs: 123,
            apps: vec![],
            warnings: vec![],
        }
    }
    fn raw_record(store: &Store, key: &str, bytes: &[u8]) {
        let txn = store.db.begin_write().unwrap();
        txn.open_table(RECORDS).unwrap().insert(key, bytes).unwrap();
        txn.commit().unwrap();
    }

    #[test]
    fn abandoned_initialization_cleans_only_owned_temporary_file() {
        let path = TestPath::new();
        let staging_path;
        {
            let staging = InitializingFile::reserve(&path.0).unwrap();
            staging_path = staging.0.clone();
            assert!(staging_path.exists());
            assert!(!path.0.exists());
            std::fs::write(&staging.0, b"failed initialization").unwrap();
            assert!(Database::create(&staging.0).is_err());
        }
        assert!(!staging_path.exists());
        assert!(!path.0.exists());
        assert!(Store::open(&path.0).is_ok());
    }

    #[test]
    fn publication_never_replaces_an_existing_file() {
        for bytes in [b"existing database bytes".as_slice(), b""] {
            let path = TestPath::new();
            std::fs::write(&path.0, bytes).unwrap();
            initialize_database(&path.0).unwrap();
            assert_eq!(std::fs::read(&path.0).unwrap(), bytes);
            assert!(Store::open(&path.0).is_err());
            assert_eq!(std::fs::read(&path.0).unwrap(), bytes);
        }
    }

    #[test]
    fn initial_io_failure_can_be_retried_without_a_stranded_final_file() {
        let directory =
            std::env::temp_dir().join(format!("kitlane-init-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("inventory.redb");
        assert!(Store::open(&path).is_err());
        assert!(!path.exists());
        std::fs::create_dir(&directory).unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(store.load_device().unwrap(), None);
        drop(store);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failed_publication_removes_staging_file_and_leaves_final_absent() {
        let directory =
            std::env::temp_dir().join(format!("kitlane-publication-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("x".repeat(300));
        assert!(initialize_database(&path).is_err());
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn concurrent_initializers_publish_one_complete_database() {
        let path = TestPath::new();
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let path = path.0.clone();
                std::thread::spawn(move || initialize_database(&path))
            })
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_settings().unwrap(), AppSettings::default());
    }

    #[cfg(unix)]
    #[test]
    fn new_database_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = TestPath::new();
        let _store = Store::open(&path.0).unwrap();
        assert_eq!(
            std::fs::metadata(&path.0).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn reopen_preserves_identity_snapshot_and_settings() {
        let path = TestPath::new();
        let device = device();
        let snapshot = snapshot(device.clone());
        let settings = AppSettings {
            dark_theme: true,
            ..Default::default()
        };
        {
            let store = Store::open(&path.0).unwrap();
            assert_eq!(store.load_device().unwrap(), None);
            assert_eq!(store.load_snapshot().unwrap(), None);
            assert_eq!(store.load_settings().unwrap(), AppSettings::default());
            store.save_device(&device).unwrap();
            store.save_snapshot(&snapshot).unwrap();
            store.save_settings(&settings).unwrap();
        }
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_device().unwrap(), Some(device));
        assert_eq!(store.load_snapshot().unwrap(), Some(snapshot));
        assert_eq!(store.load_settings().unwrap(), settings);
    }

    #[test]
    fn rejected_writes_preserve_committed_data() {
        let path = TestPath::new();
        let store = Store::open(&path.0).unwrap();
        let original = snapshot(device());
        assert!(store.save_snapshot(&original).is_err());
        store.save_device(&original.device).unwrap();
        store.save_snapshot(&original).unwrap();
        assert!(store.save_snapshot(&snapshot(device())).is_err());
        assert!(store.save_device(&device()).is_err());
        let mut invalid = original.clone();
        invalid.warnings.push("\n".into());
        assert!(store.save_snapshot(&invalid).is_err());
        drop(store);
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_snapshot().unwrap(), Some(original.clone()));
        assert_eq!(store.load_device().unwrap(), Some(original.device));
    }

    #[test]
    fn malformed_and_oversized_settings_are_not_defaults_or_overwritten() {
        for bytes in [b"{broken".to_vec(), vec![b' '; SETTINGS_LIMIT + 1]] {
            let path = TestPath::new();
            let store = Store::open(&path.0).unwrap();
            raw_record(&store, "settings", &bytes);
            assert!(store.load_settings().is_err());
            assert!(store.save_settings(&AppSettings::default()).is_err());
            let txn = store.db.begin_read().unwrap();
            assert_eq!(
                txn.open_table(RECORDS)
                    .unwrap()
                    .get("settings")
                    .unwrap()
                    .unwrap()
                    .value(),
                bytes
            );
            drop(txn);
            drop(store);
            assert!(Store::open(&path.0).is_err());
        }
    }

    #[test]
    fn incompatible_or_missing_schema_is_not_reinitialized() {
        for version in [None, Some(0), Some(SCHEMA_VERSION + 1)] {
            let path = TestPath::new();
            let store = Store::open(&path.0).unwrap();
            let txn = store.db.begin_write().unwrap();
            {
                let mut table = txn.open_table(META).unwrap();
                if let Some(version) = version {
                    table.insert("schema_version", version).unwrap();
                } else {
                    table.remove("schema_version").unwrap();
                }
            }
            txn.commit().unwrap();
            drop(store);
            assert!(Store::open(&path.0).is_err());
            let db = Database::open(&path.0).unwrap();
            let txn = db.begin_read().unwrap();
            assert_eq!(
                txn.open_table(META)
                    .unwrap()
                    .get("schema_version")
                    .unwrap()
                    .map(|v| v.value()),
                version
            );
        }
    }

    #[test]
    fn aborted_transaction_preserves_device_snapshot_and_settings() {
        let path = TestPath::new();
        let store = Store::open(&path.0).unwrap();
        let original = snapshot(device());
        store.save_device(&original.device).unwrap();
        store.save_snapshot(&original).unwrap();
        store
            .save_settings(&AppSettings {
                dark_theme: true,
                ..Default::default()
            })
            .unwrap();
        {
            let txn = store.db.begin_write().unwrap();
            let mut table = txn.open_table(RECORDS).unwrap();
            table.insert("device", b"null".as_slice()).unwrap();
            table.insert("snapshot", b"null".as_slice()).unwrap();
            table.insert("settings", b"null".as_slice()).unwrap();
            // A failure before commit drops the transaction and rolls back all records.
        }
        drop(store);
        let store = Store::open(&path.0).unwrap();
        assert_eq!(store.load_device().unwrap(), Some(original.device.clone()));
        assert_eq!(store.load_snapshot().unwrap(), Some(original));
        assert_eq!(
            store.load_settings().unwrap(),
            AppSettings {
                dark_theme: true,
                ..Default::default()
            }
        );
    }
    #[test]
    fn corrupt_and_empty_existing_files_are_preserved() {
        for bytes in [b"not a database".as_slice(), b""] {
            let path = TestPath::new();
            std::fs::write(&path.0, bytes).unwrap();
            assert!(Store::open(&path.0).is_err());
            assert_eq!(std::fs::read(&path.0).unwrap(), bytes);
        }
    }

    #[test]
    fn corrupt_inventory_device_and_conflicts_are_rejected_on_reopen() {
        for key in ["device", "snapshot"] {
            let path = TestPath::new();
            let store = Store::open(&path.0).unwrap();
            let original = snapshot(device());
            store.save_device(&original.device).unwrap();
            store.save_snapshot(&original).unwrap();
            raw_record(&store, key, b"null");
            drop(store);
            assert!(Store::open(&path.0).is_err());
        }
        let path = TestPath::new();
        let store = Store::open(&path.0).unwrap();
        store.save_device(&device()).unwrap();
        raw_record(
            &store,
            "snapshot",
            &encode(&snapshot(device()), SNAPSHOT_LIMIT).unwrap(),
        );
        assert!(store.load_snapshot().is_err());
        drop(store);
        assert!(Store::open(&path.0).is_err());
    }
}
