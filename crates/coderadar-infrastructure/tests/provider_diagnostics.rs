//! Hermetic provider health checks; no real provider credentials are used.
use coderadar_infrastructure::{JevDiagnostics, ProviderState};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn server(status: &str, body: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            bytes.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (format!("http://{address}"), handle)
}

#[test]
fn metadata_auth_and_synthetic_checks_are_distinct_and_sanitized() {
    let (url, request) = server("200 OK", r#"{"data":{"label":"configured"}}"#);
    let check = JevDiagnostics::new(&url, &url, "example", Some("test-secret"));
    assert_eq!(
        check.initial_state().state,
        ProviderState::ConfiguredUntested
    );
    let metadata = check.check_metadata();
    assert_eq!(metadata.state, ProviderState::ConfiguredUntested);
    assert!(metadata.checked_at_unix_secs.is_some());
    assert!(!format!("{metadata:?}").contains("test-secret"));
    let request = request.join().unwrap();
    assert!(request.starts_with("GET "));
    assert!(!request.contains("synthetic"));

    let (url, request) = server(
        "200 OK",
        r#"{"answers":{"probe":{"type":"noul","noul":0.7}}}"#,
    );
    let check = JevDiagnostics::new(&url, &url, "example", Some("test-secret"));
    assert_eq!(check.check_synthetic().state, ProviderState::Available);
    let request = request.join().unwrap();
    assert!(request.starts_with("POST "));
    assert!(request.contains("Synthetic health check"));
}

#[test]
fn missing_auth_protocol_and_http_failures_are_distinguished() {
    let missing = JevDiagnostics::new("http://unused", "http://unused", "example", None);
    assert_eq!(missing.initial_state().state, ProviderState::NotConfigured);
    assert_eq!(missing.check_metadata().state, ProviderState::NotConfigured);
    for (status, expected) in [
        ("401 Unauthorized", ProviderState::Unauthorized),
        ("403 Forbidden", ProviderState::Forbidden),
        ("402 Payment Required", ProviderState::QuotaBlocked),
        ("429 Too Many Requests", ProviderState::RateLimited),
        ("200 OK", ProviderState::ProtocolIncompatible),
    ] {
        let (url, request) = server(status, "not valid json");
        let check = JevDiagnostics::new(&url, &url, "example", Some("test-secret"));
        let outcome = check.check_synthetic();
        assert_eq!(outcome.state, expected);
        assert!(!format!("{outcome:?}").contains("test-secret"));
        request.join().unwrap();
    }
}

#[test]
fn metadata_requires_expected_envelope_and_synthetic_rejects_duplicate_probe() {
    let (url, request) = server("200 OK", "{}");
    assert_eq!(
        JevDiagnostics::new(&url, &url, "m", Some("k"))
            .check_metadata()
            .state,
        ProviderState::ProtocolIncompatible
    );
    request.join().unwrap();
    let (url, request) = server(
        "200 OK",
        r#"{"answers":{"probe":{"type":"noul","noul":0.2},"probe":{"type":"noul","noul":0.7}}}"#,
    );
    assert_eq!(
        JevDiagnostics::new(&url, &url, "m", Some("k"))
            .check_synthetic()
            .state,
        ProviderState::ProtocolIncompatible
    );
    request.join().unwrap();
}

#[test]
fn refuses_cleartext_remote_and_url_embedded_credentials() {
    for endpoint in [
        "http://example.com/check",
        "https://user:secret@example.com/check",
    ] {
        let check = JevDiagnostics::new(endpoint, endpoint, "m", Some("k"));
        assert_eq!(
            check.check_metadata().state,
            ProviderState::ProtocolIncompatible
        );
        assert_eq!(
            check.check_synthetic().state,
            ProviderState::ProtocolIncompatible
        );
    }
}

#[test]
fn oversized_response_is_rejected() {
    let body = format!("{{\"padding\":\"{}\"}}", "x".repeat(70_000));
    let (url, request) = server("200 OK", &body);
    let check = JevDiagnostics::new(&url, &url, "m", Some("k"));
    assert_eq!(
        check.check_metadata().state,
        ProviderState::ProtocolIncompatible
    );
    request.join().unwrap();
}

#[test]
fn synthetic_rejects_unexpected_answers_and_out_of_range_wire_values() {
    for body in [
        r#"{"answers":{"probe":{"type":"noul","noul":0.5},"extra":{"type":"noul","noul":0.5}}}"#,
        r#"{"answers":{"probe":{"type":"noul","noul":1.0000000000000002}}}"#,
        r#"{"answers":{"probe":{"type":"noul","noul":-1e-50}}}"#,
    ] {
        let (url, request) = server("200 OK", body);
        assert_eq!(
            JevDiagnostics::new(&url, &url, "m", Some("k"))
                .check_synthetic()
                .state,
            ProviderState::ProtocolIncompatible,
            "{body}"
        );
        request.join().unwrap();
    }
}
