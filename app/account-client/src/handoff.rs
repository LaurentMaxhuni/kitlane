use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub(super) fn random() -> Result<Zeroizing<String>, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Network)?;
    let result = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    Ok(Zeroizing::new(result))
}

pub(super) fn parse_callback(request: &str, host: &str, state: &str) -> Result<String, Error> {
    if request.len() > 4096 {
        return Err(Error::InvalidResponse);
    }
    let mut lines = request.split("\r\n");
    let mut start = lines.next().ok_or(Error::InvalidResponse)?.split(' ');
    if start.next() != Some("GET") {
        return Err(Error::InvalidResponse);
    }
    let path = start.next().ok_or(Error::InvalidResponse)?;
    if !matches!(start.next(), Some("HTTP/1.1") | Some("HTTP/1.0"))
        || start.next().is_some()
        || !path.starts_with("/kitlane/callback?")
    {
        return Err(Error::InvalidResponse);
    }
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("host"))
        .collect();
    if hosts.len() != 1 || hosts[0].1.trim() != host {
        return Err(Error::InvalidResponse);
    }
    let url = Url::parse(&format!("http://{host}{path}")).map_err(|_| Error::InvalidResponse)?;
    if url.path() != "/kitlane/callback" || url.fragment().is_some() {
        return Err(Error::InvalidResponse);
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if pairs.len() != 2 {
        return Err(Error::InvalidResponse);
    }
    let code = pairs
        .iter()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.as_ref())
        .ok_or(Error::InvalidResponse)?;
    let returned = pairs
        .iter()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.as_ref())
        .ok_or(Error::InvalidResponse)?;
    if !opaque(code) || returned != state {
        return Err(Error::InvalidResponse);
    }
    Ok(code.to_owned())
}

pub(super) fn read_headers(
    stream: &mut TcpStream,
    cancel: &AtomicBool,
    attempt_deadline: Instant,
    header_timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| Error::Network)?;
    let deadline = Instant::now() + header_timeout;
    let mut bytes = Zeroizing::new(Vec::new());
    let mut buffer = Zeroizing::new([0u8; 1024]);
    loop {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= attempt_deadline {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline || bytes.len() > 4096 {
            return Err(Error::InvalidResponse);
        }
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(bytes);
        }
        match stream.read(buffer.as_mut()) {
            Ok(0) => return Err(Error::InvalidResponse),
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(Error::InvalidResponse),
        }
    }
}

pub(super) fn sign_in(client: &Client, cancel: &AtomicBool) -> Result<Credentials, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| Error::Network)?;
    listener.set_nonblocking(true).map_err(|_| Error::Network)?;
    let host = listener
        .local_addr()
        .map_err(|_| Error::Network)?
        .to_string();
    let state = random()?;
    let verifier = random()?;
    let proof = client.installation_proof()?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Attempt {
        browser_url: String,
        expires_at: u64,
    }
    let pending:Attempt=Client::decode(client.request(Method::POST,"/v2/auth/attempts",Some(serde_json::json!({"state":state.as_str(),"challenge":URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),"callback":format!("http://{host}/kitlane/callback"),"device":client.device,"device_proof":proof.as_str()})),false)?,8192)?;
    let browser = Url::parse(&pending.browser_url).map_err(|_| Error::InvalidResponse)?;
    if browser.origin().ascii_serialization() != client.endpoint
        || browser.path() != "/sign-in"
        || !browser.username().is_empty()
        || browser.password().is_some()
        || browser.fragment().is_some()
        || pending.expires_at <= now()
        || pending.expires_at > now() + 360
    {
        return Err(Error::InvalidResponse);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    webbrowser::open(browser.as_str()).map_err(|_| Error::Network)?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed)
            || started.elapsed() > Duration::from_secs(300)
            || now() >= pending.expires_at
        {
            return Err(Error::Cancelled);
        }
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                stream
                    .set_write_timeout(Some(Duration::from_millis(500)))
                    .map_err(|_| Error::Network)?;
                let bytes = match read_headers(
                    &mut stream,
                    cancel,
                    started + Duration::from_secs(300),
                    Duration::from_secs(2),
                ) {
                    Ok(bytes) => bytes,
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(_) => continue,
                };
                let code = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|request| parse_callback(request, &host, &state).ok())
                    .map(Zeroizing::new);
                let (status, body) = if code.is_some() {
                    (
                        "200 OK",
                        "You can return to Kitlane. This window can be closed.",
                    )
                } else {
                    (
                        "400 Bad Request",
                        "This callback is unavailable. Return to Kitlane and try again.",
                    )
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                if let Some(code) = code {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(Error::Cancelled);
                    }
                    return Client::decode(client.request(Method::POST,"/v2/auth/exchange",Some(serde_json::json!({"code":code.as_str(),"verifier":verifier.as_str(),"device_id":client.device.id})),false)?,8192);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err(Error::Network),
        }
    }
}
