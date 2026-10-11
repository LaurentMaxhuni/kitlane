use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use kitlane_core::model::{DeviceInfo, InventorySnapshot};
use rand_core::{OsRng, TryRngCore};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::Path as FilePath,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_DEVICES: usize = 64;
const MAX_GRANTS: usize = 64;
const GRANT_LIFETIME_SECS: u64 = 300;
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const DEVICES: TableDefinition<&str, &[u8]> = TableDefinition::new("devices");
const GRANTS: TableDefinition<&str, &[u8]> = TableDefinition::new("pairing_grants");
const SNAPSHOTS: TableDefinition<&str, &[u8]> = TableDefinition::new("snapshots");

#[derive(Clone)]
pub struct Service {
    db: Arc<Database>,
    requests: Arc<tokio::sync::Semaphore>,
    blocking_work: Arc<tokio::sync::Semaphore>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceRecord {
    device: DeviceInfo,
    token_hash: [u8; 32],
    revoked: bool,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    device: DeviceInfo,
    token_hash: [u8; 32],
    issued_at: u64,
    expires_at: u64,
    #[serde(default)]
    rotate_credential: bool,
    #[serde(default)]
    previous_token_hash: Option<[u8; 32]>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotUpdate {
    pub revision: u64,
    pub snapshot: InventorySnapshot,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairingRequest {
    device: DeviceInfo,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentRequest {
    pairing_token: String,
}
#[derive(Serialize)]
struct PairingResponse {
    pairing_token: String,
    expires_at_unix_secs: u64,
    device: DeviceInfo,
}
#[derive(Serialize)]
struct EnrollmentResponse {
    device_token: String,
    device: DeviceInfo,
}
#[derive(Serialize)]
struct DeviceSummary {
    device: DeviceInfo,
    revision: u64,
}

#[derive(Clone, Copy)]
enum Principal {
    Owner,
    Device,
}

// Error messages are fixed: never reflect tokens, request bodies, or database contents.
#[derive(Debug)]
pub struct ApiError(StatusCode, &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error": self.1}))).into_response()
    }
}
fn internal(_: impl std::fmt::Display) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Service storage or entropy unavailable",
    )
}
fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Invalid or revoked credential")
}
fn bad(message: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message)
}
fn conflict(message: &'static str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message)
}
fn now() -> Result<u64, ApiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(internal)
}
fn token_hash(token: &str) -> Result<[u8; 32], ApiError> {
    if token.len() != 64 {
        return Err(unauthorized());
    }
    let bytes = hex::decode(token).map_err(|_| unauthorized())?;
    if bytes.len() != 32 || token.len() != 64 {
        return Err(unauthorized());
    }
    Ok(Sha256::digest(bytes).into())
}
fn random_token() -> Result<String, ApiError> {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes).map_err(internal)?;
    Ok(hex::encode(bytes))
}
fn bearer(headers: &HeaderMap) -> Result<[u8; 32], ApiError> {
    if headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
        return Err(unauthorized());
    }
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    token_hash(value)
}
fn decode<T: serde::de::DeserializeOwned>(value: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(value).map_err(internal)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ApiError> {
    serde_json::to_vec(value).map_err(internal)
}
fn authenticate(tx: &WriteTransaction, hash: &[u8; 32]) -> Result<Principal, ApiError> {
    let table = tx.open_table(META).map_err(internal)?;
    let owner = table
        .get("owner_hash")
        .map_err(internal)?
        .ok_or_else(|| internal("Missing owner"))?;
    if bool::from(owner.value().ct_eq(hash)) {
        return Ok(Principal::Owner);
    }
    let devices = tx.open_table(DEVICES).map_err(internal)?;
    let mut authorized = false;
    for item in devices.iter().map_err(internal)? {
        let (_, record) = item.map_err(internal)?;
        let record: DeviceRecord = decode(record.value())?;
        authorized |= bool::from(record.token_hash.ct_eq(hash)) && !record.revoked;
    }
    if authorized {
        Ok(Principal::Device)
    } else {
        Err(unauthorized())
    }
}
fn require_owner(tx: &WriteTransaction, hash: &[u8; 32]) -> Result<(), ApiError> {
    match authenticate(tx, hash)? {
        Principal::Owner => Ok(()),
        Principal::Device => Err(ApiError(StatusCode::FORBIDDEN, "Owner credential required")),
    }
}

// Staging owns only a random, exclusively reserved sibling file. A crash before
// publication can leave this orphan, but never an incomplete final database.
struct InitializingFile(std::path::PathBuf);
impl InitializingFile {
    fn reserve(path: &FilePath) -> Result<Self, ApiError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(FilePath::new("."));
        let temporary = parent.join(format!(".kitlane-sync-init-{}.redb", random_token()?));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&temporary).map_err(internal)?;
        Ok(Self(temporary))
    }
}
impl Drop for InitializingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn initialize_database(path: &FilePath, owner_hash: &[u8; 32]) -> Result<(), ApiError> {
    let temporary = InitializingFile::reserve(path)?;
    {
        let db = Database::create(&temporary.0).map_err(internal)?;
        let tx = db.begin_write().map_err(internal)?;
        {
            let mut meta = tx.open_table(META).map_err(internal)?;
            meta.insert("schema", b"1".as_slice()).map_err(internal)?;
            meta.insert("owner_hash", owner_hash.as_slice())
                .map_err(internal)?;
            tx.open_table(DEVICES).map_err(internal)?;
            tx.open_table(GRANTS).map_err(internal)?;
            tx.open_table(SNAPSHOTS).map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(&temporary.0)
        .map_err(internal)?
        .sync_all()
        .map_err(internal)?;
    // Exclusive publication cannot replace a competing initializer or unrelated file.
    match std::fs::hard_link(&temporary.0, path) {
        Ok(()) => {
            #[cfg(unix)]
            {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(FilePath::new("."));
                std::fs::File::open(parent)
                    .map_err(internal)?
                    .sync_all()
                    .map_err(internal)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(internal(error)),
    }
}

impl Service {
    /// Owner token must encode 32 cryptographically random bytes as 64 hex characters.
    /// Its hash is pinned on first creation; a different owner cannot adopt an existing DB.
    pub fn open(path: impl AsRef<FilePath>, owner_token: &str) -> Result<Self, ApiError> {
        let hash = token_hash(owner_token)
            .map_err(|_| bad("Owner token must encode 32 random bytes as 64 hex characters"))?;
        let path = path.as_ref();
        if !path.try_exists().map_err(internal)? {
            initialize_database(path, &hash)?;
        }
        let db = Database::open(path).map_err(internal)?;
        {
            let tx = db.begin_read().map_err(internal)?;
            let meta = tx.open_table(META).map_err(internal)?;
            let version = meta
                .get("schema")
                .map_err(internal)?
                .ok_or_else(|| bad("Existing database lacks service schema"))?;
            if version.value() != b"1" {
                return Err(bad("Unsupported service database schema"));
            }
            let owner = meta
                .get("owner_hash")
                .map_err(internal)?
                .ok_or_else(|| internal("Missing owner"))?;
            if !bool::from(owner.value().ct_eq(&hash)) {
                return Err(bad("Configured owner does not match this database"));
            }
            // Opening missing tables must fail, rather than silently repairing lost auth state.
            tx.open_table(DEVICES).map_err(internal)?;
            tx.open_table(GRANTS).map_err(internal)?;
            tx.open_table(SNAPSHOTS).map_err(internal)?;
        }
        Ok(Self {
            db: Arc::new(db),
            requests: Arc::new(tokio::sync::Semaphore::new(16)),
            blocking_work: Arc::new(tokio::sync::Semaphore::new(16)),
        })
    }
    async fn transaction<T: Send + 'static>(
        &self,
        action: impl FnOnce(&WriteTransaction) -> Result<T, ApiError> + Send + 'static,
    ) -> Result<T, ApiError> {
        let permit = self
            .blocking_work
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                ApiError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Storage work capacity reached",
                )
            })?;
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            // Canceled HTTP callers cannot release capacity while database work continues.
            let _permit = permit;
            let tx = db.begin_write().map_err(internal)?;
            let result = action(&tx)?;
            tx.commit().map_err(internal)?;
            Ok(result)
        })
        .await
        .map_err(internal)?
    }
}

pub fn router(service: Service) -> Router {
    let protected = Router::new()
        .route("/v1/pairing-grants", post(pair))
        .route("/v1/devices", get(roster))
        .route("/v1/devices/{id}", delete(revoke))
        .route("/v1/devices/{id}/pairing-grants", post(recovery_pair))
        .route("/v1/snapshots/{id}", get(read_snapshot).put(write_snapshot))
        .route_layer(middleware::from_fn_with_state(service.clone(), auth_guard));
    Router::new()
        .merge(protected)
        .route("/v1/enroll", post(enroll))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(service.clone(), admission))
        .layer(middleware::map_response(no_store))
        .with_state(service)
}
async fn admission(
    State(service): State<Service>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let _permit = service
        .requests
        .try_acquire()
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "Request capacity reached"))?;
    Ok(next.run(request).await)
}
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
async fn auth_guard(
    State(service): State<Service>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let hash = bearer(request.headers())?;
    service
        .transaction(move |tx| authenticate(tx, &hash))
        .await?;
    Ok(next.run(request).await)
}
async fn pair(
    State(service): State<Service>,
    headers: HeaderMap,
    Json(request): Json<PairingRequest>,
) -> Result<Json<PairingResponse>, ApiError> {
    let hash = bearer(&headers)?;
    request.device.validate().map_err(bad)?;
    let id =
        uuid::Uuid::parse_str(&request.device.id).map_err(|_| bad("Invalid device identity"))?;
    if id.is_nil() || id.to_string() != request.device.id {
        return Err(bad("Device identity must be a non-nil canonical UUID"));
    }
    service
        .transaction(move |tx| {
            require_owner(tx, &hash)?;
            let devices = tx.open_table(DEVICES).map_err(internal)?;
            if devices
                .get(request.device.id.as_str())
                .map_err(internal)?
                .is_some()
            {
                return Err(conflict(
                    "Device identity already enrolled; use a new identity",
                ));
            }
            if devices.iter().map_err(internal)?.count() >= MAX_DEVICES {
                return Err(conflict("Device capacity reached"));
            }
            issue_grant(tx, request.device, None)
        })
        .await
}

async fn recovery_pair(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<PairingResponse>, ApiError> {
    let hash = bearer(&headers)?;
    service
        .transaction(move |tx| {
            require_owner(tx, &hash)?;
            if !body.is_empty() {
                return Err(bad("Recovery pairing requires an empty body"));
            }
            let devices = tx.open_table(DEVICES).map_err(internal)?;
            let record: DeviceRecord = devices
                .get(id.as_str())
                .map_err(internal)?
                .map(|value| decode(value.value()))
                .transpose()?
                .ok_or(ApiError(StatusCode::NOT_FOUND, "Device not found"))?;
            if record.revoked {
                return Err(conflict("Revoked device cannot recover credentials"));
            }
            issue_grant(tx, record.device, Some(record.token_hash))
        })
        .await
}

fn issue_grant(
    tx: &WriteTransaction,
    device: DeviceInfo,
    previous_token_hash: Option<[u8; 32]>,
) -> Result<Json<PairingResponse>, ApiError> {
    let timestamp = now()?;
    let mut grants = tx.open_table(GRANTS).map_err(internal)?;
    // Expired grants are removed without ever storing their cleartext credentials.
    let mut expired = Vec::new();
    for item in grants.iter().map_err(internal)? {
        let (key, value) = item.map_err(internal)?;
        let grant: Grant = decode(value.value())?;
        if grant.expires_at <= timestamp || timestamp < grant.issued_at {
            expired.push(key.value().to_owned());
        }
    }
    for key in expired {
        grants.remove(key.as_str()).map_err(internal)?;
    }
    if grants.iter().map_err(internal)?.count() >= MAX_GRANTS {
        return Err(conflict("Pairing capacity reached"));
    }
    let token = random_token()?;
    let hash = token_hash(&token)?;
    let grant = Grant {
        device: device.clone(),
        token_hash: hash,
        issued_at: timestamp,
        expires_at: timestamp + GRANT_LIFETIME_SECS,
        rotate_credential: previous_token_hash.is_some(),
        previous_token_hash,
    };
    grants
        .insert(hex::encode(hash).as_str(), encode(&grant)?.as_slice())
        .map_err(internal)?;
    Ok(Json(PairingResponse {
        pairing_token: token,
        expires_at_unix_secs: grant.expires_at,
        device,
    }))
}
async fn enroll(
    State(service): State<Service>,
    Json(request): Json<EnrollmentRequest>,
) -> Result<(StatusCode, Json<EnrollmentResponse>), ApiError> {
    let hash = token_hash(&request.pairing_token)?;
    service
        .transaction(move |tx| {
            let timestamp = now()?;
            let mut grants = tx.open_table(GRANTS).map_err(internal)?;
            let mut matched = None;
            for item in grants.iter().map_err(internal)? {
                let (key, value) = item.map_err(internal)?;
                let grant: Grant = decode(value.value())?;
                if bool::from(grant.token_hash.ct_eq(&hash))
                    && timestamp >= grant.issued_at
                    && timestamp < grant.expires_at
                {
                    matched = Some((key.value().to_owned(), grant));
                }
            }
            let (key, grant) = matched.ok_or_else(unauthorized)?;
            let mut devices = tx.open_table(DEVICES).map_err(internal)?;
            let existing: Option<DeviceRecord> = devices
                .get(grant.device.id.as_str())
                .map_err(internal)?
                .map(|value| decode(value.value()))
                .transpose()?;
            let mut record = if grant.rotate_credential {
                match existing {
                    Some(record)
                        if !record.revoked
                            && record.device == grant.device
                            && grant.previous_token_hash.is_some_and(|previous| {
                                bool::from(previous.ct_eq(&record.token_hash))
                            }) =>
                    {
                        record
                    }
                    _ => {
                        // Stale/revoked recovery grants are consumed and rejected in the same commit.
                        grants.remove(key.as_str()).map_err(internal)?;
                        return Ok(Err(conflict(
                            "Recovery grant no longer matches active device credential",
                        )));
                    }
                }
            } else {
                if existing.is_some() {
                    return Err(conflict("Device identity already enrolled"));
                }
                if devices.iter().map_err(internal)?.count() >= MAX_DEVICES {
                    return Err(conflict("Device capacity reached"));
                }
                DeviceRecord {
                    device: grant.device.clone(),
                    token_hash: [0; 32],
                    revoked: false,
                    revision: 0,
                }
            };
            let token = random_token()?;
            record.token_hash = token_hash(&token)?;
            devices
                .insert(grant.device.id.as_str(), encode(&record)?.as_slice())
                .map_err(internal)?;
            grants.remove(key.as_str()).map_err(internal)?;
            Ok(Ok((
                StatusCode::CREATED,
                Json(EnrollmentResponse {
                    device_token: token,
                    device: grant.device,
                }),
            )))
        })
        .await?
}
async fn roster(
    State(service): State<Service>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeviceSummary>>, ApiError> {
    let hash = bearer(&headers)?;
    service
        .transaction(move |tx| {
            authenticate(tx, &hash)?;
            let table = tx.open_table(DEVICES).map_err(internal)?;
            let mut result = Vec::new();
            for item in table.iter().map_err(internal)? {
                let (_, value) = item.map_err(internal)?;
                let record: DeviceRecord = decode(value.value())?;
                if !record.revoked {
                    result.push(DeviceSummary {
                        device: record.device,
                        revision: record.revision,
                    });
                }
            }
            Ok(Json(result))
        })
        .await
}
async fn revoke(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let hash = bearer(&headers)?;
    service
        .transaction(move |tx| {
            require_owner(tx, &hash)?;
            let mut devices = tx.open_table(DEVICES).map_err(internal)?;
            let mut record: DeviceRecord = devices
                .get(id.as_str())
                .map_err(internal)?
                .map(|v| decode(v.value()))
                .transpose()?
                .ok_or(ApiError(StatusCode::NOT_FOUND, "Device not found"))?;
            record.revoked = true;
            devices
                .insert(id.as_str(), encode(&record)?.as_slice())
                .map_err(internal)?;
            tx.open_table(SNAPSHOTS)
                .map_err(internal)?
                .remove(id.as_str())
                .map_err(internal)?;
            Ok(StatusCode::NO_CONTENT)
        })
        .await
}
async fn write_snapshot(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(update): Json<SnapshotUpdate>,
) -> Result<StatusCode, ApiError> {
    let hash = bearer(&headers)?;
    update.snapshot.validate().map_err(bad)?;
    if update.snapshot.device.id != id {
        return Err(bad("Snapshot and route device identity must match"));
    }
    service
        .transaction(move |tx| {
            if let Principal::Owner = authenticate(tx, &hash)? {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "Enrolled device credential required",
                ));
            }
            let mut devices = tx.open_table(DEVICES).map_err(internal)?;
            let mut record: DeviceRecord = devices
                .get(id.as_str())
                .map_err(internal)?
                .map(|v| decode(v.value()))
                .transpose()?
                .ok_or(ApiError(
                    StatusCode::FORBIDDEN,
                    "Device cannot write this identity",
                ))?;
            if record.revoked || !bool::from(record.token_hash.ct_eq(&hash)) {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "Device cannot write this identity",
                ));
            }
            if update.snapshot.device != record.device {
                return Err(bad("Snapshot metadata must match enrolled identity"));
            }
            if update.revision
                != record
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| conflict("Revision exhausted"))?
            {
                return Err(conflict("Revision must equal stored revision plus one"));
            }
            record.revision = update.revision;
            devices
                .insert(id.as_str(), encode(&record)?.as_slice())
                .map_err(internal)?;
            tx.open_table(SNAPSHOTS)
                .map_err(internal)?
                .insert(id.as_str(), encode(&update)?.as_slice())
                .map_err(internal)?;
            Ok(StatusCode::NO_CONTENT)
        })
        .await
}
async fn read_snapshot(
    State(service): State<Service>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<SnapshotUpdate>, ApiError> {
    let hash = bearer(&headers)?;
    service
        .transaction(move |tx| {
            authenticate(tx, &hash)?;
            let table = tx.open_table(SNAPSHOTS).map_err(internal)?;
            let snapshot = table
                .get(id.as_str())
                .map_err(internal)?
                .map(|v| decode(v.value()))
                .transpose()?
                .ok_or(ApiError(StatusCode::NOT_FOUND, "Snapshot not found"))?;
            Ok(Json(snapshot))
        })
        .await
}

#[cfg(test)]
mod tests;
