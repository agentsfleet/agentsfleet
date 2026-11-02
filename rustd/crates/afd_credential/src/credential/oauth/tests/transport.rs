#![expect(
    clippy::expect_used,
    reason = "the one-shot socket is a deterministic transport fixture"
)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::thread;

use jiff::Timestamp;
use serde_json::json;
use zeroize::Zeroizing;

use super::super::{Refresh, mint};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::OauthApp;

const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

struct Endpoint {
    url: String,
    task: thread::JoinHandle<()>,
}

fn serve(status: Option<&str>, body: &[u8], advertised_len: usize) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server binds");
    let address = listener.local_addr().expect("fixture address resolves");
    let status = status.map(str::to_owned);
    let body = body.to_vec();
    let task = thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("fixture request arrives");
        read_request(&mut socket);
        let Some(status) = status else { return };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {advertised_len}\r\nConnection: close\r\n\r\n"
        );
        socket
            .write_all(head.as_bytes())
            .expect("fixture response head writes");
        socket.write_all(&body).expect("fixture body writes");
    });
    Endpoint {
        url: format!("http://{address}/token"),
        task,
    }
}

// Closing with unread POST bytes can reset the socket before the client reads
// the intended response. Consume the entire form before choosing the fault.
fn read_request(socket: &mut TcpStream) {
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    let mut length = None;
    loop {
        line.clear();
        assert!(reader.read_line(&mut line).expect("fixture headers read") > 0);
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse::<usize>().expect("form length is valid"));
        }
    }
    let mut body = vec![0; length.expect("the encoded form declares its length")];
    reader
        .read_exact(&mut body)
        .expect("the complete form arrives");
    assert!(
        String::from_utf8(body)
            .expect("the encoded form is UTF-8")
            .contains("grant_type=refresh_token")
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "the client dials only the loopback token endpoint each test stands up"
)]
async fn exchange(endpoint: Endpoint) -> Outcome {
    let app = OauthApp {
        client_id: "client".to_owned(),
        client_secret: Zeroizing::new("secret".to_owned()),
    };
    let outcome = mint(Refresh {
        app: &app,
        handle: &json!({"refresh_token": "old-refresh"}),
        token_url: &endpoint.url,
        http: &reqwest::Client::new(),
        now: NOW,
    })
    .await;
    endpoint
        .task
        .join()
        .expect("the fixture completed its request");
    outcome
}

#[tokio::test]
async fn a_complete_success_response_becomes_the_minted_credential() {
    let body = br#"{"access_token":"fresh","expires_in":60,"refresh_token":"replacement"}"#;
    let outcome = exchange(serve(Some("200 OK"), body, body.len())).await;
    let minted = outcome.minted().expect("a complete response mints");

    assert_eq!(minted.token.as_str(), "fresh");
    assert_eq!(minted.expires_at, Timestamp::constant(1_760_000_060, 0));
    assert_eq!(
        minted.rotated_refresh_token.as_deref().map(String::as_str),
        Some("replacement")
    );
}

#[tokio::test]
async fn response_statuses_retain_reconnect_and_retry_classification() {
    let invalid = br#"{"error":"invalid_grant"}"#;
    assert!(matches!(
        exchange(serve(Some("401 Unauthorized"), invalid, invalid.len())).await,
        Outcome::ReconnectRequired
    ));

    let unavailable = br#"{"error":"temporarily_unavailable"}"#;
    assert!(matches!(
        exchange(serve(
            Some("503 Service Unavailable"),
            unavailable,
            unavailable.len()
        ))
        .await,
        Outcome::MintFailed(Retry::Transient)
    ));

    let malformed = b"not-json";
    assert!(matches!(
        exchange(serve(Some("200 OK"), malformed, malformed.len())).await,
        Outcome::MintFailed(Retry::Permanent)
    ));
}

#[tokio::test]
async fn transport_and_incomplete_body_failures_remain_retryable() {
    assert!(matches!(
        exchange(serve(None, b"", 0)).await,
        Outcome::MintFailed(Retry::Transient)
    ));

    assert!(matches!(
        exchange(serve(Some("200 OK"), b"{}", 100)).await,
        Outcome::MintFailed(Retry::Transient)
    ));
}
