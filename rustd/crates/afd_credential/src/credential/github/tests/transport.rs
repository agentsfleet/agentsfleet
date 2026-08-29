#![expect(
    clippy::expect_used,
    reason = "the one-shot socket and client are transport fixture preconditions"
)]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::thread;

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use octocrab::Octocrab;

use super::super::exchange::request_token;
use crate::credential::outcome::{Outcome, Retry};

const NOW_MS: i64 = 1_760_000_000_000;

fn serve(status: &str, body: &[u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server binds");
    let address = listener.local_addr().expect("fixture address resolves");
    let status = status.to_owned();
    let body = body.to_vec();
    thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("fixture request arrives");
        let mut request = [0_u8; 4096];
        let _read = socket.read(&mut request).expect("fixture request reads");
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket
            .write_all(head.as_bytes())
            .expect("fixture response head writes");
        socket.write_all(&body).expect("fixture body writes");
    });
    format!("http://{address}")
}

fn client(base: &str) -> Octocrab {
    Octocrab::builder()
        .base_uri(base)
        .expect("fixture base is a URI")
        .build()
        .expect("fixture client builds")
}

fn binding() -> RepositoryBinding {
    RepositoryBinding::from_parts(vec!["acme/widgets".into()], Access::Read, None)
}

#[tokio::test]
async fn a_narrowed_response_is_delivered_with_the_local_expiry_ceiling() {
    let body = br#"{"token":"ghs_fixture","expires_at":"2026-01-01T00:00:00Z","permissions":{"contents":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let outcome = request_token(&client(&serve("201 Created", body)), 42, &binding(), NOW_MS).await;
    let minted = outcome.minted().expect("the narrow response mints");

    assert_eq!(minted.token.as_str(), "ghs_fixture");
    assert_eq!(minted.expires_at_ms, NOW_MS + 3_600_000);
    assert!(minted.rotated_refresh_token.is_none());
}

#[tokio::test]
async fn a_successful_but_overreaching_response_is_discarded() {
    let body = br#"{"token":"ghs_fixture","permissions":{"contents":"write"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let outcome = request_token(&client(&serve("201 Created", body)), 42, &binding(), NOW_MS).await;

    assert!(matches!(outcome, Outcome::MintFailed(Retry::Permanent)));
}

#[tokio::test]
async fn vendor_status_and_decode_failures_keep_their_retry_posture() {
    for status in ["401 Unauthorized", "404 Not Found"] {
        let outcome = request_token(
            &client(&serve(status, br#"{"message":"gone"}"#)),
            42,
            &binding(),
            NOW_MS,
        )
        .await;
        assert!(matches!(outcome, Outcome::ReconnectRequired));
    }

    let unavailable = request_token(
        &client(&serve("503 Service Unavailable", br#"{"message":"later"}"#)),
        42,
        &binding(),
        NOW_MS,
    )
    .await;
    assert!(matches!(unavailable, Outcome::MintFailed(Retry::Transient)));

    let malformed = request_token(
        &client(&serve("201 Created", b"not-json")),
        42,
        &binding(),
        NOW_MS,
    )
    .await;
    assert!(matches!(malformed, Outcome::MintFailed(Retry::Transient)));
}
