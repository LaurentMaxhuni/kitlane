use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use kitlane_core::model::Platform;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const OWNER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OTHER: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

fn fixture() -> (TempDir, Service) {
    let dir = TempDir::new().unwrap();
    let service = Service::open(dir.path().join("service.redb"), OWNER).unwrap();
    (dir, service)
}
fn device(n: u32) -> DeviceInfo {
    DeviceInfo {
        id: format!("00000000-0000-4000-8000-{n:012}"),
        name: format!("Device {n}"),
        platform: Platform::Windows,
        architecture: "x86_64".into(),
    }
}
fn update(device: &DeviceInfo, revision: u64) -> Value {
    json!({"revision": revision, "snapshot": {"device": device, "scanned_at_unix_secs": 1, "apps": [], "warnings": []}})
}
async fn raw_request(
    app: Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: String,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES + 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}
async fn request(
    service: &Service,
    method: &str,
    path: &str,
    token: Option<&str>,
    value: Value,
) -> (StatusCode, Value) {
    raw_request(
        router(service.clone()),
        method,
        path,
        token,
        value.to_string(),
    )
    .await
}
async fn grant(service: &Service, device: &DeviceInfo) -> String {
    let (status, value) = request(
        service,
        "POST",
        "/v1/pairing-grants",
        Some(OWNER),
        json!({"device":device}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    value["pairing_token"].as_str().unwrap().to_owned()
}
async fn enroll_device(service: &Service, device: &DeviceInfo) -> String {
    let pairing = grant(service, device).await;
    let (status, value) = request(
        service,
        "POST",
        "/v1/enroll",
        None,
        json!({"pairing_token":pairing}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    value["device_token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn unauthenticated_bad_tokens_and_owner_device_roles() {
    let (_dir, service) = fixture();
    for token in [None, Some("bad"), Some(OTHER)] {
        assert_eq!(
            request(&service, "GET", "/v1/devices", token, Value::Null)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(
                &service,
                "POST",
                "/v1/pairing-grants",
                token,
                json!({"device":device(1)})
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":OTHER})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let d = device(1);
    let token = enroll_device(&service, &d).await;
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(&token),
            json!({"device":device(2)})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &service,
            "DELETE",
            &format!("/v1/devices/{}", d.id),
            Some(&token),
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &service,
            "PUT",
            &format!("/v1/snapshots/{}", d.id),
            Some(OWNER),
            update(&d, 1)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&service, "GET", "/v1/devices", Some(&token), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn pairing_is_single_use_even_concurrently_and_expires() {
    let (_dir, service) = fixture();
    let token = grant(&service, &device(1)).await;
    let enrollment = json!({"pairing_token": token});
    let (a, b) = tokio::join!(
        request(&service, "POST", "/v1/enroll", None, enrollment.clone()),
        request(&service, "POST", "/v1/enroll", None, enrollment)
    );
    let mut statuses = [a.0.as_u16(), b.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [201, 401]);
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(OWNER),
            json!({"device": device(1)})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let expired = grant(&service, &device(2)).await;
    let key = hex::encode(token_hash(&expired).unwrap());
    service
        .transaction(move |tx| {
            let mut table = tx.open_table(GRANTS).map_err(internal)?;
            let mut grant: Grant =
                decode(table.get(key.as_str()).map_err(internal)?.unwrap().value())?;
            grant.issued_at = 0;
            grant.expires_at = now()?;
            table
                .insert(key.as_str(), encode(&grant)?.as_slice())
                .map_err(internal)?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":expired})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn revisions_cross_device_metadata_validation_and_body_limit() {
    let (_dir, service) = fixture();
    let d1 = device(1);
    let d2 = device(2);
    let token1 = enroll_device(&service, &d1).await;
    let token2 = enroll_device(&service, &d2).await;
    let path = format!("/v1/snapshots/{}", d1.id);
    assert_eq!(
        request(&service, "PUT", &path, Some(&token2), update(&d1, 1))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), update(&d2, 1))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut changed = d1.clone();
    changed.name = "Changed identity".into();
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), update(&changed, 1))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), update(&d1, 0))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), update(&d1, 1))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    for revision in [0, 1, 3] {
        assert_eq!(
            request(&service, "PUT", &path, Some(&token1), update(&d1, revision))
                .await
                .0,
            StatusCode::CONFLICT
        );
    }
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), update(&d1, 2))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let read = request(&service, "GET", &path, Some(&token2), Value::Null).await;
    assert_eq!(read.0, StatusCode::OK);
    assert_eq!(read.1["revision"], 2);
    let mut invalid = update(&d1, 3);
    invalid["snapshot"]["warnings"] = json!(["\n"]);
    assert_eq!(
        request(&service, "PUT", &path, Some(&token1), invalid)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let body = format!("{{\"padding\":\"{}\"}}", "x".repeat(MAX_BODY_BYTES));
    assert_eq!(
        raw_request(router(service.clone()), "PUT", &path, Some(&token1), body)
            .await
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        request(&service, "GET", &path, Some(&token1), Value::Null)
            .await
            .1["revision"],
        2
    );
}

#[tokio::test]
async fn restart_preserves_grants_tokens_snapshots_revisions_and_revocation() {
    let (dir, service) = fixture();
    let d = device(1);
    let token = enroll_device(&service, &d).await;
    let pending = grant(&service, &device(2)).await;
    let path = format!("/v1/snapshots/{}", d.id);
    assert_eq!(
        request(&service, "PUT", &path, Some(&token), update(&d, 1))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    drop(service);
    let dbpath = dir.path().join("service.redb");
    assert!(Service::open(&dbpath, OTHER).is_err());
    let service = Service::open(&dbpath, OWNER).unwrap();
    assert_eq!(
        request(&service, "GET", &path, Some(&token), Value::Null)
            .await
            .1["revision"],
        1
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&token), update(&d, 1))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pending})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        request(
            &service,
            "DELETE",
            &format!("/v1/devices/{}", d.id),
            Some(OWNER),
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    drop(service);
    let service = Service::open(&dbpath, OWNER).unwrap();
    assert_eq!(
        request(&service, "GET", "/v1/devices", Some(&token), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&token), update(&d, 2))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&service, "GET", &path, Some(OWNER), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(OWNER),
            json!({"device": d})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[test]
fn owner_configuration_rejects_wrong_format_and_database_contains_only_hashes() {
    let (dir, service) = fixture();
    assert!(Service::open(dir.path().join("invalid.redb"), "short").is_err());
    let tx = service.db.begin_write().unwrap();
    let table = tx.open_table(META).unwrap();
    let hash = table.get("owner_hash").unwrap().unwrap();
    assert_eq!(hash.value(), token_hash(OWNER).unwrap());
    assert_ne!(hash.value(), OWNER.as_bytes());
}

#[tokio::test]
async fn canonical_identity_pairing_and_request_capacity() {
    let (_dir, service) = fixture();
    for id in [
        "00000000-0000-0000-0000-000000000000",
        "AAAAAAAA-0000-4000-8000-000000000001",
    ] {
        let mut d = device(1);
        d.id = id.into();
        assert_eq!(
            request(
                &service,
                "POST",
                "/v1/pairing-grants",
                Some(OWNER),
                json!({"device":d})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for _ in 0..MAX_GRANTS {
        grant(&service, &device(1)).await;
    }
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(OWNER),
            json!({"device":device(2)})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let permits = service.requests.acquire_many(16).await.unwrap();
    assert_eq!(
        request(&service, "GET", "/v1/devices", Some(OWNER), Value::Null)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(permits);
    assert_eq!(
        request(&service, "GET", "/v1/devices", Some(OWNER), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
}

#[test]
fn existing_schemaless_and_unrelated_files_fail_closed_without_adoption() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.redb");
    {
        let db = Database::create(&path).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(META).unwrap();
            table
                .insert("owner_hash", token_hash(OTHER).unwrap().as_slice())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    assert!(Service::open(&path, OWNER).is_err());
    {
        let db = Database::open(&path).unwrap();
        let tx = db.begin_write().unwrap();
        let table = tx.open_table(META).unwrap();
        assert!(table.get("schema").unwrap().is_none());
        assert_eq!(
            table.get("owner_hash").unwrap().unwrap().value(),
            token_hash(OTHER).unwrap()
        );
    }
    let unrelated = dir.path().join("unrelated.txt");
    std::fs::write(&unrelated, "Keep unrelated data").unwrap();
    assert!(Service::open(&unrelated, OWNER).is_err());
    assert_eq!(
        std::fs::read_to_string(&unrelated).unwrap(),
        "Keep unrelated data"
    );
}

#[test]
fn initialization_retries_after_abandoned_staging_and_preserves_publication_winner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("service.redb");
    let staging = InitializingFile::reserve(&path).unwrap();
    std::fs::write(&staging.0, "Interrupted initialization").unwrap();
    assert!(!path.exists());
    let service = Service::open(&path, OWNER).unwrap();
    assert!(path.exists());
    assert_eq!(
        std::fs::read_to_string(&staging.0).unwrap(),
        "Interrupted initialization"
    );
    drop(staging);
    drop(service);
    // A competing initializer cannot overwrite an already published owner.
    initialize_database(&path, &token_hash(OTHER).unwrap()).unwrap();
    assert!(Service::open(&path, OTHER).is_err());
    assert!(Service::open(&path, OWNER).is_ok());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn canceled_caller_cannot_release_running_database_work_capacity() {
    let (_dir, mut service) = fixture();
    service.blocking_work = Arc::new(tokio::sync::Semaphore::new(1));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let caller_service = service.clone();
    let caller = tokio::spawn(async move {
        caller_service
            .transaction(move |_| {
                entered_tx
                    .send(())
                    .map_err(|_| internal("Test receiver closed"))?;
                release_rx.recv().map_err(internal)?;
                Ok(())
            })
            .await
    });
    entered_rx.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    let result = service.transaction(|_| Ok(())).await;
    release_tx.send(()).unwrap();
    assert_eq!(result.unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
}

async fn recovery_grant(service: &Service, device: &DeviceInfo) -> String {
    let path = format!("/v1/devices/{}/pairing-grants", device.id);
    let (status, response) = raw_request(
        router(service.clone()),
        "POST",
        &path,
        Some(OWNER),
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["device"], json!(device));
    response["pairing_token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn recovery_rotates_credentials_preserves_state_and_recovers_lost_response() {
    let (dir, service) = fixture();
    let mut d = device(1);
    d.name = "Unchanged owner-approved metadata".into();
    let old_token = enroll_device(&service, &d).await;
    let path = format!("/v1/snapshots/{}", d.id);
    assert_eq!(
        request(&service, "PUT", &path, Some(&old_token), update(&d, 1))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let initial_snapshot = request(&service, "GET", &path, Some(OWNER), Value::Null)
        .await
        .1;
    let pairing = recovery_grant(&service, &d).await;
    // Issuing a grant alone does not invalidate the installed credential.
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&old_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, response) = request(
        &service,
        "POST",
        "/v1/enroll",
        None,
        json!({"pairing_token":pairing}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(response["device"], json!(d));
    let new_token = response["device_token"].as_str().unwrap().to_owned();
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&old_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&service, "GET", &path, Some(&new_token), Value::Null)
            .await
            .1,
        initial_snapshot
    );
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&new_token),
            Value::Null
        )
        .await
        .1[0]["revision"],
        1
    );
    assert_eq!(
        request(&service, "PUT", &path, Some(&new_token), update(&d, 2))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let fresh = recovery_grant(&service, &d).await;
    drop(service);
    let service = Service::open(dir.path().join("service.redb"), OWNER).unwrap();
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pairing})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&service, "GET", &path, Some(&new_token), Value::Null)
            .await
            .1["revision"],
        2
    );
    // Even if the previous enrollment response was lost or secure storage failed,
    // the owner can issue a fresh recovery grant for the same stable identity.
    let (status, response) = request(
        &service,
        "POST",
        "/v1/enroll",
        None,
        json!({"pairing_token":fresh}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let recovered_token = response["device_token"].as_str().unwrap();
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&new_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&service, "GET", &path, Some(recovered_token), Value::Null)
            .await
            .1["revision"],
        2
    );
}

#[tokio::test]
async fn recovery_is_owner_only_and_revocation_invalidates_pending_grants() {
    let (_dir, service) = fixture();
    let d = device(1);
    let old_token = enroll_device(&service, &d).await;
    let path = format!("/v1/devices/{}/pairing-grants", d.id);
    for token in [None, Some(OTHER)] {
        assert_eq!(
            raw_request(router(service.clone()), "POST", &path, token, String::new())
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        raw_request(
            router(service.clone()),
            "POST",
            &path,
            Some(&old_token),
            String::new()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        raw_request(
            router(service.clone()),
            "POST",
            &path,
            Some(OWNER),
            "{}".into()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let missing = format!("/v1/devices/{}/pairing-grants", device(2).id);
    assert_eq!(
        raw_request(
            router(service.clone()),
            "POST",
            &missing,
            Some(OWNER),
            String::new()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let pairing = recovery_grant(&service, &d).await;
    assert_eq!(
        request(
            &service,
            "DELETE",
            &format!("/v1/devices/{}", d.id),
            Some(OWNER),
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pairing})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pairing})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw_request(
            router(service.clone()),
            "POST",
            &path,
            Some(OWNER),
            String::new()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(OWNER),
            json!({"device":d})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&old_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn concurrent_recovery_grants_cannot_overwrite_a_newer_credential_generation() {
    let (_dir, service) = fixture();
    let d = device(1);
    let old_token = enroll_device(&service, &d).await;
    let (a, b) = tokio::join!(recovery_grant(&service, &d), recovery_grant(&service, &d));
    let (result_a, result_b) = tokio::join!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":a})
        ),
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":b})
        )
    );
    let mut statuses = [result_a.0.as_u16(), result_b.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [201, 409]);
    let winner = if result_a.0 == StatusCode::CREATED {
        result_a.1
    } else {
        result_b.1
    };
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(winner["device_token"].as_str().unwrap()),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&old_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for token in [a, b] {
        assert_eq!(
            request(
                &service,
                "POST",
                "/v1/enroll",
                None,
                json!({"pairing_token":token})
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn older_persisted_pairing_grants_default_to_initial_enrollment() {
    let (dir, service) = fixture();
    let d = device(1);
    let pairing = grant(&service, &d).await;
    let key = hex::encode(token_hash(&pairing).unwrap());
    service
        .transaction(move |tx| {
            let mut grants = tx.open_table(GRANTS).map_err(internal)?;
            let mut original: Value =
                decode(grants.get(key.as_str()).map_err(internal)?.unwrap().value())?;
            original
                .as_object_mut()
                .unwrap()
                .remove("rotate_credential");
            original
                .as_object_mut()
                .unwrap()
                .remove("previous_token_hash");
            grants
                .insert(key.as_str(), encode(&original)?.as_slice())
                .map_err(internal)?;
            Ok(())
        })
        .await
        .unwrap();
    drop(service);
    let service = Service::open(dir.path().join("service.redb"), OWNER).unwrap();
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pairing})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/pairing-grants",
            Some(OWNER),
            json!({"device":d})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn expected_device_mismatch_or_invalid_identity_does_not_consume_enrollment_grant() {
    let (_dir, service) = fixture();
    let d = device(1);
    let pairing = grant(&service, &d).await;
    let mut wrong_id = d.clone();
    wrong_id.id = device(2).id;
    let mut wrong_name = d.clone();
    wrong_name.name = "Wrong device".into();
    let mut wrong_architecture = d.clone();
    wrong_architecture.architecture = "aarch64".into();
    let mut wrong_platform = d.clone();
    wrong_platform.platform = Platform::Linux;
    let mut malformed_id = d.clone();
    malformed_id.id = "not-a-uuid".into();
    for expected in [
        wrong_id,
        wrong_name,
        wrong_architecture,
        wrong_platform,
        malformed_id,
    ] {
        assert_eq!(
            request(
                &service,
                "POST",
                "/v1/enroll",
                None,
                json!({"pairing_token":pairing, "expected_device":expected})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(&service, "GET", "/v1/devices", Some(OWNER), Value::Null)
            .await
            .1,
        json!([])
    );
    let (status, response) = request(
        &service,
        "POST",
        "/v1/enroll",
        None,
        json!({"pairing_token":pairing, "expected_device":d}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(response["device"], json!(d));
}

#[tokio::test]
async fn recovery_expected_device_mismatch_preserves_token_and_grant_until_correct_retry() {
    let (_dir, service) = fixture();
    let d = device(1);
    let old_token = enroll_device(&service, &d).await;
    let snapshot_path = format!("/v1/snapshots/{}", d.id);
    assert_eq!(
        request(
            &service,
            "PUT",
            &snapshot_path,
            Some(&old_token),
            update(&d, 1)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let pairing = recovery_grant(&service, &d).await;
    let mut wrong = d.clone();
    wrong.name = "Unexpected metadata".into();
    assert_eq!(
        request(
            &service,
            "POST",
            "/v1/enroll",
            None,
            json!({"pairing_token":pairing, "expected_device":wrong})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &service,
            "GET",
            &snapshot_path,
            Some(&old_token),
            Value::Null
        )
        .await
        .1,
        update(&d, 1)
    );
    let (status, response) = request(
        &service,
        "POST",
        "/v1/enroll",
        None,
        json!({"pairing_token":pairing, "expected_device":d}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        request(
            &service,
            "GET",
            "/v1/devices",
            Some(&old_token),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &service,
            "GET",
            &snapshot_path,
            Some(response["device_token"].as_str().unwrap()),
            Value::Null
        )
        .await
        .1,
        update(&d, 1)
    );
}
