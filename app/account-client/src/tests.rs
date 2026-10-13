use super::*;
use std::{collections::HashMap, io::Write, net::TcpListener, sync::Mutex, thread};

#[derive(Default)]
struct MemoryVault(Mutex<HashMap<String, String>>);
impl Vault for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<String>, kitlane_sync_client::Error> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<(), kitlane_sync_client::Error> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), kitlane_sync_client::Error> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
fn device() -> DeviceInfo {
    DeviceInfo {
        id: kitlane_core::new_device_id(),
        name: "Test".into(),
        platform: kitlane_core::model::Platform::Windows,
        architecture: "x86_64".into(),
    }
}

#[test]
fn requires_https_origin_and_unambiguous_codes() {
    for value in [
        "http://127.0.0.1:8787",
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com?token=secret",
    ] {
        assert!(matches!(
            Client::with_vault(value, &device(), Arc::new(MemoryVault::default())),
            Err(Error::Configuration)
        ));
    }
    assert!(valid_code("ABCD2345"));
    for c in ["ABCD01IO", "ABCD23456", "abcd2345"] {
        assert!(!valid_code(c));
    }
}
#[test]
fn callback_rejects_wrong_state_host_path_and_duplicate_codes() {
    let state = "a".repeat(43);
    let code = "b".repeat(43);
    let host = "127.0.0.1:49152";
    let request =
        format!("GET /kitlane/callback?code={code}&state={state} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    assert_eq!(
        handoff::parse_callback(&request, host, &state).unwrap(),
        code
    );
    for wrong in [
        request.replace(&state, &"c".repeat(43)),
        request.replace(host, "evil.example"),
        request.replace("/kitlane/callback", "/elsewhere"),
        request.replace("HTTP/1.1", &format!("&code={code} HTTP/1.1")),
    ] {
        assert!(handoff::parse_callback(&wrong, host, &state).is_err());
    }
}
#[test]
fn responses_are_bounded_and_redirects_never_forward_credentials() {
    let vault = Arc::new(MemoryVault::default());
    let mut client = Client::with_vault("https://account.example", &device(), vault).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    client.endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        let count = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET /v2/account"));
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://foreign.example\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert!(matches!(
        client.request(Method::GET, "/v2/account", None, false),
        Err(Error::InvalidResponse)
    ));
    handle.join().unwrap();
    assert!(!Error::Network.to_string().contains("foreign"));
}
#[test]
fn invalid_session_cannot_unlock_and_local_signout_forgets_credentials() {
    let vault = Arc::new(MemoryVault::default());
    let mut client =
        Client::with_vault("https://account.example", &device(), vault.clone()).unwrap();
    let c = Credentials {
        access_token: "secret".into(),
        refresh_token: "secret".into(),
        device_id: client.device.id.clone(),
        account: Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        },
        expires_at: now() + 900,
    };
    assert!(matches!(client.accept(c), Err(Error::InvalidResponse)));
    assert!(client.credentials.is_none());
    vault.set(&client.key, "saved").unwrap();
    client.sign_out().unwrap();
    assert!(vault.get(&client.key).unwrap().is_none());
}

#[test]
fn vault_write_failure_never_unlocks_session() {
    struct LockedVault;
    impl Vault for LockedVault {
        fn get(&self, _: &str) -> Result<Option<String>, kitlane_sync_client::Error> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), kitlane_sync_client::Error> {
            Err(kitlane_sync_client::Error::VaultUnavailable)
        }
        fn delete(&self, _: &str) -> Result<(), kitlane_sync_client::Error> {
            Ok(())
        }
    }
    let mut client =
        Client::with_vault("https://account.example", &device(), Arc::new(LockedVault)).unwrap();
    let credentials = Credentials {
        access_token: "a".repeat(43),
        refresh_token: "b".repeat(43),
        device_id: client.device.id.clone(),
        account: Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        },
        expires_at: now() + 900,
    };
    assert!(matches!(client.accept(credentials), Err(Error::Vault)));
    assert!(client.credentials.is_none());
}

#[test]
fn revoked_refresh_forgets_saved_credentials_and_oversized_responses_are_rejected() {
    let vault = Arc::new(MemoryVault::default());
    let mut client =
        Client::with_vault("https://account.example", &device(), vault.clone()).unwrap();
    let credentials = Credentials {
        access_token: "a".repeat(43),
        refresh_token: "b".repeat(43),
        device_id: client.device.id.clone(),
        account: Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        },
        expires_at: now() + 900,
    };
    client.accept(credentials).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    client.endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        for response in [
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n{}",
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0; 8192];
            assert!(stream.read(&mut bytes).unwrap() > 0);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    assert!(client.restore().unwrap().is_none());
    assert!(vault.get(&client.key).unwrap().is_none());
    assert!(client.credentials.is_none());
    let response = client
        .request(Method::GET, "/v2/account", None, false)
        .unwrap();
    assert!(matches!(
        Client::decode::<serde_json::Value>(response, 1024),
        Err(Error::InvalidResponse)
    ));
    handle.join().unwrap();
}

#[test]
fn idle_signout_revokes_using_refresh_proof_even_when_access_expired() {
    let vault = Arc::new(MemoryVault::default());
    let mut client =
        Client::with_vault("https://account.example", &device(), vault.clone()).unwrap();
    let credentials = Credentials {
        access_token: "a".repeat(43),
        refresh_token: "b".repeat(43),
        device_id: client.device.id.clone(),
        account: Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        },
        expires_at: now() + 900,
    };
    client.accept(credentials).unwrap();
    client.credentials.as_mut().unwrap().expires_at = now() - 1;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    client.endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 8192];
        let count = stream.read(&mut bytes).unwrap();
        let request = String::from_utf8_lossy(&bytes[..count]);
        assert!(request.starts_with("POST /v2/auth/revoke"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .unwrap();
    });
    assert!(client.sign_out().is_ok());
    handle.join().unwrap();
    assert!(client.credentials.is_none());
    assert!(vault.get(&client.key).unwrap().is_none());
}

#[test]
fn trickling_callback_headers_have_absolute_deadlines_and_can_be_cancelled() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    for cancelled in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let reader_flag = flag.clone();
        let reader = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let started = Instant::now();
            let result = handoff::read_headers(
                &mut stream,
                &reader_flag,
                Instant::now() + Duration::from_secs(5),
                Duration::from_millis(200),
            );
            assert!(result.is_err());
            assert!(started.elapsed() < Duration::from_millis(700));
        });
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        for n in 0..10 {
            if stream.write_all(b"G").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
            if cancelled && n == 1 {
                flag.store(true, Ordering::Relaxed);
            }
        }
        drop(stream);
        reader.join().unwrap();
    }
}

#[test]
fn expiry_and_vault_failures_leave_no_usable_session() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct FlakyVault {
        memory: MemoryVault,
        fail_set: AtomicBool,
        fail_delete: AtomicBool,
    }
    impl Vault for FlakyVault {
        fn get(&self, key: &str) -> Result<Option<String>, kitlane_sync_client::Error> {
            self.memory.get(key)
        }
        fn set(&self, key: &str, value: &str) -> Result<(), kitlane_sync_client::Error> {
            if self.fail_set.load(Ordering::Relaxed) {
                Err(kitlane_sync_client::Error::VaultUnavailable)
            } else {
                self.memory.set(key, value)
            }
        }
        fn delete(&self, key: &str) -> Result<(), kitlane_sync_client::Error> {
            if self.fail_delete.load(Ordering::Relaxed) {
                Err(kitlane_sync_client::Error::VaultUnavailable)
            } else {
                self.memory.delete(key)
            }
        }
    }
    for revoked in [false, true] {
        let vault = Arc::new(FlakyVault {
            memory: MemoryVault::default(),
            fail_set: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
        });
        let mut client =
            Client::with_vault("https://account.example", &device(), vault.clone()).unwrap();
        let credentials = Credentials {
            access_token: "a".repeat(43),
            refresh_token: "b".repeat(43),
            device_id: client.device.id.clone(),
            account: Account {
                account_id: kitlane_core::new_device_id(),
                email: "test@example.com".into(),
                name: "Test".into(),
            },
            expires_at: now() + 900,
        };
        let body = serde_json::to_string(&credentials).unwrap();
        client.accept(credentials).unwrap();
        client.credentials.as_mut().unwrap().expires_at = now() - 1;
        assert!(!client.session_valid());
        vault.fail_set.store(!revoked, Ordering::Relaxed);
        vault.fail_delete.store(revoked, Ordering::Relaxed);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        client.endpoint = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 8192];
            assert!(stream.read(&mut request).unwrap() > 0);
            let response = if revoked {
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
            };
            stream.write_all(response.as_bytes()).unwrap();
        });
        assert!(matches!(client.maintain_session(), Err(Error::Vault)));
        assert!(!client.session_valid());
        assert!(client.credentials.is_none());
        handle.join().unwrap();
    }
}

#[test]
fn installation_proof_survives_account_signout_and_metadata_update_uses_authenticated_api() {
    let vault = Arc::new(MemoryVault::default());
    let mut client =
        Client::with_vault("https://account.example", &device(), vault.clone()).unwrap();
    let proof = client.installation_proof().unwrap();
    client.sign_out().unwrap();
    assert_eq!(
        proof.as_str(),
        client.installation_proof().unwrap().as_str()
    );
    let credentials = Credentials {
        access_token: "a".repeat(43),
        refresh_token: "b".repeat(43),
        device_id: client.device.id.clone(),
        account: Account {
            account_id: kitlane_core::new_device_id(),
            email: "test@example.com".into(),
            name: "Test".into(),
        },
        expires_at: now() + 900,
    };
    client.accept(credentials).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    client.endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 8192];
        let count = stream.read(&mut bytes).unwrap();
        let request = String::from_utf8_lossy(&bytes[..count]);
        assert!(request.starts_with("PUT /v2/device"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer")
        );
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .unwrap();
    });
    let mut changed = client.device.clone();
    changed.name = "Chosen in onboarding".into();
    client.update_device(&changed).unwrap();
    handle.join().unwrap();
    assert_eq!(client.device.name, changed.name);
}
