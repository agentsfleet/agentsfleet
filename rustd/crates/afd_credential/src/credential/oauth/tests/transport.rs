#![expect(
    clippy::expect_used,
    reason = "the one-shot socket is a deterministic transport fixture"
)]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::thread;

use serde_json::json;
use zeroize::Zeroizing;

use super::super::{Refresh, mint};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::OauthApp;

const NOW_MS: i64 = 1_760_000_000_000;

fn serve(status: &str, body: &[u8], advertised_len: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server binds");
    let address = listener.local_addr().expect("fixture address resolves");
    let status = status.to_owned();
    let body = body.to_vec();
    thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("fixture request arrives");
        let mut request = [0_u8; 4096];
        let _read = socket.read(&mut request).expect("fixture request reads");
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {advertised_len}\r\nConnection: close\r\n\r\n"
        );
        socket
            .write_all(head.as_bytes())
            .expect("fixture response head writes");
        socket.write_all(&body).expect("fixture body writes");
    });
    format!("http://{address}/token")
}

async fn exchange(url: &str) -> Outcome {
    let app = OauthApp {
        client_id: "client".to_owned(),
        client_secret: Zeroizing::new("secret".to_owned()),
    };
    mint(Refresh {
        app: &app,
        handle: &json!({"refresh_token": "old-refresh"}),
        token_url: url,
        http: &reqwest::Client::new(),
        now_ms: NOW_MS,
    })
    .await
}

#[tokio::test]
async fn a_complete_success_response_becomes_the_minted_credential() {
    let body = br#"{"access_token":"fresh","expires_in":60,"refresh_token":"replacement"}"#;
    let outcome = exchange(&serve("200 OK", body, body.len())).await;
    let minted = outcome.minted().expect("a complete response mints");

    assert_eq!(minted.token.as_str(), "fresh");
    assert_eq!(minted.expires_at_ms, NOW_MS + 60_000);
    assert_eq!(
        minted.rotated_refresh_token.as_deref().map(String::as_str),
        Some("replacement")
    );
}

#[tokio::test]
async fn response_statuses_retain_reconnect_and_retry_classification() {
    let invalid = br#"{"error":"invalid_grant"}"#;
    assert!(matches!(
        exchange(&serve("401 Unauthorized", invalid, invalid.len())).await,
        Outcome::ReconnectRequired
    ));

    let unavailable = br#"{"error":"temporarily_unavailable"}"#;
    assert!(matches!(
        exchange(&serve(
            "503 Service Unavailable",
            unavailable,
            unavailable.len()
        ))
        .await,
        Outcome::MintFailed(Retry::Transient)
    ));

    let malformed = b"not-json";
    assert!(matches!(
        exchange(&serve("200 OK", malformed, malformed.len())).await,
        Outcome::MintFailed(Retry::Permanent)
    ));
}

#[tokio::test]
async fn transport_and_incomplete_body_failures_remain_retryable() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture port binds");
    let refused = format!(
        "http://{}/token",
        listener.local_addr().expect("fixture address resolves")
    );
    drop(listener);
    assert!(matches!(
        exchange(&refused).await,
        Outcome::MintFailed(Retry::Transient)
    ));

    assert!(matches!(
        exchange(&serve("200 OK", b"{}", 100)).await,
        Outcome::MintFailed(Retry::Transient)
    ));
}
