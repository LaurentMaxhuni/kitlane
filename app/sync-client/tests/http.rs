use kitlane_core::model::{DeviceInfo, InventorySnapshot, Platform};
use kitlane_sync_client::{Client, Error, Vault};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const PAIR: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOKEN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
#[derive(Default)]
struct MemoryVault {
    tokens: Mutex<HashMap<String, String>>,
    fail_set: bool,
    fail_get: bool,
}
impl Vault for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<String>, Error> {
        if self.fail_get {
            return Err(Error::VaultUnavailable);
        }
        Ok(self.tokens.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, token: &str) -> Result<(), Error> {
        if self.fail_set {
            return Err(Error::VaultUnavailable);
        }
        self.tokens.lock().unwrap().insert(key.into(), token.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), Error> {
        self.tokens.lock().unwrap().remove(key);
        Ok(())
    }
}
fn device() -> DeviceInfo {
    DeviceInfo {
        id: "9c1f7c60-e081-4e69-b073-7f82d2c925fb".into(),
        name: "Workstation".into(),
        platform: Platform::Windows,
        architecture: "x86_64".into(),
    }
}
fn snapshot(device: DeviceInfo, time: u64) -> InventorySnapshot {
    InventorySnapshot {
        device,
        scanned_at_unix_secs: time,
        apps: vec![],
        warnings: vec![],
    }
}
struct Step {
    method: &'static str,
    path: String,
    status: u16,
    body: String,
    auth: bool,
    revision: Option<u64>,
    redirect: bool,
    drop_response: bool,
}
impl Step {
    fn json(method: &'static str, path: &str, status: u16, body: Value) -> Self {
        Self {
            method,
            path: path.into(),
            status,
            body: body.to_string(),
            auth: method != "POST",
            revision: None,
            redirect: false,
            drop_response: false,
        }
    }
    fn enroll(device: DeviceInfo) -> Self {
        Self::json(
            "POST",
            "/v1/enroll",
            201,
            json!({"device_token":TOKEN,"device":device}),
        )
    }
}
fn request(stream: &mut TcpStream) -> (String, String) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut data = Vec::new();
    let header_end = loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        data.push(byte[0]);
        if data.ends_with(b"\r\n\r\n") {
            break data.len();
        }
        assert!(data.len() < 16384);
    };
    let headers = String::from_utf8(data[..header_end].to_vec()).unwrap();
    let len = headers
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(0);
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    (headers, String::from_utf8(body).unwrap())
}
fn fake(steps: Vec<Step>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let thread = thread::spawn(move || {
        for step in steps {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = request(&mut stream);
            assert!(headers.starts_with(&format!("{} {} HTTP/1.1", step.method, step.path)));
            if step.auth {
                assert!(
                    headers
                        .to_lowercase()
                        .contains(&format!("authorization: bearer {TOKEN}"))
                );
            } else {
                assert!(!headers.to_lowercase().contains("authorization:"));
                assert_eq!(
                    serde_json::from_str::<Value>(&body).unwrap()["pairing_token"],
                    PAIR
                );
            }
            if let Some(revision) = step.revision {
                assert_eq!(
                    serde_json::from_str::<Value>(&body).unwrap()["revision"],
                    revision
                );
            }
            if step.drop_response {
                continue;
            }
            let location = if step.redirect {
                "Location: http://127.0.0.1:1/credential-sink\r\n"
            } else {
                ""
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
                step.status,
                step.body.len(),
                location,
                step.body
            );
        }
    });
    (endpoint, thread)
}
fn enrolled_client(steps: Vec<Step>) -> (Client, thread::JoinHandle<()>) {
    let mut full = vec![Step::enroll(device())];
    full.extend(steps);
    let (endpoint, thread) = fake(full);
    let client =
        Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    client.enroll(PAIR).unwrap();
    (client, thread)
}

#[test]
fn exact_loopback_transport_and_identity_validation() {
    let vault = Arc::new(MemoryVault::default());
    for endpoint in [
        "http://localhost:8787",
        "http://127.1:8787",
        "http://2130706433:8787",
        "https://127.0.0.1:8787",
        "http://127.0.0.1:0",
        "http://127.0.0.1:65536",
        "http://127.0.0.1:8787/",
        "http://127.0.0.1:8787/path",
        "http://127.0.0.1:8787?token=x",
        "http://127.0.0.1:8787#x",
        "http://user@127.0.0.1:8787",
        " http://127.0.0.1:8787",
        "http://127.0.0.1:8787\\x",
        "http://127.0.0.1:8787\n",
    ] {
        assert!(matches!(
            Client::with_vault(endpoint, &device(), vault.clone()),
            Err(Error::InvalidEndpoint)
        ));
    }
    let mut bad = device();
    bad.id = "00000000-0000-0000-0000-000000000000".into();
    assert!(matches!(
        Client::with_vault("http://127.0.0.1:8787", &bad, vault),
        Err(Error::InvalidDevice)
    ));
}
#[test]
fn vault_namespaces_and_local_forgetting() {
    let (endpoint, server) = fake(vec![Step::enroll(device())]);
    let vault = Arc::new(MemoryVault::default());
    let client = Client::with_vault(&endpoint, &device(), vault.clone()).unwrap();
    assert!(!client.is_enrolled().unwrap());
    client.enroll(PAIR).unwrap();
    assert!(client.is_enrolled().unwrap());
    assert_eq!(client.enroll(PAIR), Err(Error::AlreadyEnrolled));
    assert!(
        Client::with_vault(&endpoint, &device(), vault.clone())
            .unwrap()
            .is_enrolled()
            .unwrap()
    );
    assert!(
        !Client::with_vault("http://127.0.0.1:1", &device(), vault.clone())
            .unwrap()
            .is_enrolled()
            .unwrap()
    );
    let mut different = device();
    different.id = "cb68245b-12c1-4cc9-a4f1-1008b9e6c5df".into();
    assert!(
        !Client::with_vault(&endpoint, &different, vault)
            .unwrap()
            .is_enrolled()
            .unwrap()
    );
    client.disconnect().unwrap();
    client.disconnect().unwrap();
    assert!(!client.is_enrolled().unwrap());
    server.join().unwrap();
}
#[test]
fn enrollment_checks_metadata_and_reports_consumed_grant_without_retry() {
    let mut other = device();
    other.name = "Wrong".into();
    let (endpoint, server) = fake(vec![Step::enroll(other)]);
    let client =
        Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(client.enroll(PAIR), Err(Error::EnrollmentUncertain));
    assert!(!client.is_enrolled().unwrap());
    server.join().unwrap();
    let (endpoint, server) = fake(vec![Step::enroll(device())]);
    let client = Client::with_vault(
        &endpoint,
        &device(),
        Arc::new(MemoryVault {
            fail_set: true,
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(client.enroll(PAIR), Err(Error::EnrollmentVaultFailed));
    server.join().unwrap();
}
#[test]
fn enrollment_rejects_invalid_grants_and_unavailable_vault_before_network() {
    let client = Client::with_vault(
        "http://127.0.0.1:1",
        &device(),
        Arc::new(MemoryVault::default()),
    )
    .unwrap();
    assert_eq!(client.enroll("123456"), Err(Error::InvalidPairingToken));
    assert!(matches!(
        client.sync(&snapshot(device(), 0)),
        Err(Error::NotEnrolled)
    ));
    let client = Client::with_vault(
        "http://127.0.0.1:1",
        &device(),
        Arc::new(MemoryVault {
            fail_get: true,
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(client.enroll(PAIR), Err(Error::VaultUnavailable));
}
#[test]
fn redirects_and_server_messages_cannot_escape_or_leak() {
    let mut step = Step::json("POST", "/v1/enroll", 302, json!({"error":PAIR}));
    step.redirect = true;
    let (endpoint, server) = fake(vec![step]);
    let client =
        Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    let err = client.enroll(PAIR).unwrap_err();
    assert_eq!(err, Error::InvalidResponse);
    assert!(!format!("{err} {err:?}").contains(PAIR));
    server.join().unwrap();
}
#[test]
fn lost_enrollment_response_is_uncertain_and_sent_once() {
    let mut step = Step::enroll(device());
    step.drop_response = true;
    let (endpoint, server) = fake(vec![step]);
    let client =
        Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(client.enroll(PAIR), Err(Error::EnrollmentUncertain));
    server.join().unwrap();
}
#[test]
fn sync_recovers_committed_ambiguous_write_and_excludes_self() {
    let local = snapshot(device(), 10);
    let mut peer = device();
    peer.id = "cb68245b-12c1-4cc9-a4f1-1008b9e6c5df".into();
    peer.name = "Peer".into();
    let peer_snapshot = snapshot(peer.clone(), 20);
    let steps = vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":1},{"device":peer,"revision":3}]),
        ),
        Step::json(
            "GET",
            &format!("/v1/snapshots/{}", device().id),
            200,
            json!({"revision":1,"snapshot":local}),
        ),
        Step::json(
            "GET",
            &format!("/v1/snapshots/{}", peer.id),
            200,
            json!({"revision":3,"snapshot":peer_snapshot}),
        ),
    ];
    let (client, server) = enrolled_client(steps);
    let result = client.sync(&local).unwrap();
    assert_eq!(result.peers, vec![peer_snapshot]);
    server.join().unwrap();
}
#[test]
fn sync_reads_current_revision_then_writes_exact_next_revision() {
    let mut put = Step::json(
        "PUT",
        &format!("/v1/snapshots/{}", device().id),
        204,
        Value::Null,
    );
    put.body.clear();
    put.revision = Some(8);
    let steps = vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":7}]),
        ),
        Step::json(
            "GET",
            &format!("/v1/snapshots/{}", device().id),
            200,
            json!({"revision":7,"snapshot":snapshot(device(),1)}),
        ),
        put,
    ];
    let (client, server) = enrolled_client(steps);
    assert!(client.sync(&snapshot(device(), 2)).is_ok());
    server.join().unwrap();
}
#[test]
fn sync_never_retries_conflicting_upload_or_revoked_token() {
    let mut put = Step::json(
        "PUT",
        &format!("/v1/snapshots/{}", device().id),
        409,
        json!({"error":TOKEN}),
    );
    put.revision = Some(1);
    let (client, server) = enrolled_client(vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":0}]),
        ),
        put,
    ]);
    assert!(matches!(
        client.sync(&snapshot(device(), 1)),
        Err(Error::Conflict)
    ));
    server.join().unwrap();
    let (client, server) = enrolled_client(vec![Step::json(
        "GET",
        "/v1/devices",
        401,
        json!({"error":TOKEN}),
    )]);
    assert!(matches!(
        client.sync(&snapshot(device(), 1)),
        Err(Error::Unauthorized)
    ));
    assert!(client.is_enrolled().unwrap());
    server.join().unwrap();
}
#[test]
fn rejects_duplicate_roster_metadata_changes_and_revision_races_before_upload() {
    let roster = json!([{"device":device(),"revision":0},{"device":device(),"revision":0}]);
    let (client, server) = enrolled_client(vec![Step::json("GET", "/v1/devices", 200, roster)]);
    assert!(matches!(
        client.sync(&snapshot(device(), 1)),
        Err(Error::InvalidResponse)
    ));
    server.join().unwrap();
    let mut wrong = snapshot(device(), 1);
    wrong.device.name = "Different".into();
    let (client, server) = enrolled_client(vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":1}]),
        ),
        Step::json(
            "GET",
            &format!("/v1/snapshots/{}", device().id),
            200,
            json!({"revision":1,"snapshot":wrong}),
        ),
    ]);
    assert!(matches!(
        client.sync(&snapshot(device(), 2)),
        Err(Error::InvalidResponse)
    ));
    server.join().unwrap();
    let (client, server) = enrolled_client(vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":1}]),
        ),
        Step::json(
            "GET",
            &format!("/v1/snapshots/{}", device().id),
            200,
            json!({"revision":2,"snapshot":snapshot(device(),1)}),
        ),
    ]);
    assert!(matches!(
        client.sync(&snapshot(device(), 2)),
        Err(Error::Conflict)
    ));
    server.join().unwrap();
}
#[test]
fn bounds_response_and_roster() {
    let mut huge = Step::json("GET", "/v1/devices", 200, Value::Null);
    huge.body = " ".repeat(2 * 1024 * 1024 + 1);
    let (client, server) = enrolled_client(vec![huge]);
    assert!(matches!(
        client.sync(&snapshot(device(), 1)),
        Err(Error::TooLarge)
    ));
    server.join().unwrap();
    let roster = vec![json!({"device":device(),"revision":0}); 65];
    let (client, server) =
        enrolled_client(vec![Step::json("GET", "/v1/devices", 200, json!(roster))]);
    assert!(matches!(
        client.sync(&snapshot(device(), 1)),
        Err(Error::TooLarge)
    ));
    server.join().unwrap();
}
#[test]
fn old_wire_inventory_and_optional_native_metadata_remain_compatible() {
    let mut value = json!(snapshot(device(), 1));
    value["apps"] = json!([{"id":"a","name":"A","publisher":null,"version":null,"scope":"user","source":"flatpak"}]);
    let old: InventorySnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(old.apps[0].native_id, None);
    value["apps"][0]["native_id"] = json!("org.example.App");
    value["apps"][0]["origin"] = json!("flathub");
    let new: InventorySnapshot = serde_json::from_value(value).unwrap();
    assert!(new.validate().is_ok());
}

#[test]
fn real_service_two_enrolled_devices_revision_update_and_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let service =
        kitlane_sync_server::Service::open(directory.path().join("test.redb"), PAIR).unwrap();
    let (ready_send, ready_recv) = std::sync::mpsc::channel();
    let (stop_send, stop_recv) = tokio::sync::oneshot::channel::<()>();
    let server = thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                ready_send
                    .send(format!("http://{}", listener.local_addr().unwrap()))
                    .unwrap();
                axum::serve(listener, kitlane_sync_server::router(service))
                    .with_graceful_shutdown(async {
                        let _ = stop_recv.await;
                    })
                    .await
                    .unwrap();
            });
    });
    let endpoint = ready_recv.recv_timeout(Duration::from_secs(5)).unwrap();
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let grant = |device: &DeviceInfo| -> String {
        let response: Value = http
            .post(format!("{endpoint}/v1/pairing-grants"))
            .bearer_auth(PAIR)
            .header("content-type", "application/json")
            .body(json!({"device":device}).to_string())
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap();
        response["pairing_token"].as_str().unwrap().to_owned()
    };
    let first = Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    first.enroll(&grant(&device())).unwrap();
    assert!(first.sync(&snapshot(device(), 1)).unwrap().peers.is_empty());
    let mut peer = device();
    peer.id = "cb68245b-12c1-4cc9-a4f1-1008b9e6c5df".into();
    peer.name = "Peer".into();
    let second = Client::with_vault(&endpoint, &peer, Arc::new(MemoryVault::default())).unwrap();
    second.enroll(&grant(&peer)).unwrap();
    let first_inventory = snapshot(device(), 1);
    assert_eq!(
        second.sync(&snapshot(peer.clone(), 2)).unwrap().peers,
        vec![first_inventory]
    );
    assert_eq!(
        first.sync(&snapshot(device(), 3)).unwrap().peers,
        vec![snapshot(peer, 2)]
    );
    // Repeating identical inventory must not increment the stored revision.
    first.sync(&snapshot(device(), 3)).unwrap();
    let roster: Value = http
        .get(format!("{endpoint}/v1/devices"))
        .bearer_auth(PAIR)
        .send()
        .unwrap()
        .text()
        .map(|text| serde_json::from_str(&text).unwrap())
        .unwrap();
    assert_eq!(
        roster
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["device"]["id"] == device().id)
            .unwrap()["revision"],
        2
    );
    assert_eq!(
        http.delete(format!("{endpoint}/v1/devices/{}", device().id))
            .bearer_auth(PAIR)
            .send()
            .unwrap()
            .status()
            .as_u16(),
        204
    );
    assert!(matches!(
        first.sync(&snapshot(device(), 4)),
        Err(Error::Unauthorized)
    ));
    stop_send.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn ambiguous_put_is_read_back_before_any_new_write() {
    let local = snapshot(device(), 10);
    let path = format!("/v1/snapshots/{}", device().id);
    let mut put = Step::json("PUT", &path, 204, Value::Null);
    put.drop_response = true;
    put.revision = Some(1);
    let (client, server) = enrolled_client(vec![
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":0}]),
        ),
        put,
        Step::json(
            "GET",
            "/v1/devices",
            200,
            json!([{"device":device(),"revision":1}]),
        ),
        Step::json("GET", &path, 200, json!({"revision":1,"snapshot":local})),
    ]);
    assert!(matches!(client.sync(&local), Err(Error::Unavailable)));
    assert!(client.sync(&local).unwrap().peers.is_empty());
    server.join().unwrap();
}

#[test]
fn enrollment_server_failure_requires_owner_check() {
    let (endpoint, server) = fake(vec![Step::json(
        "POST",
        "/v1/enroll",
        500,
        json!({"error":PAIR}),
    )]);
    let client =
        Client::with_vault(&endpoint, &device(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(client.enroll(PAIR), Err(Error::EnrollmentUncertain));
    server.join().unwrap();
}
