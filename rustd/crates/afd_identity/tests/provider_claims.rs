//! Reading a capability claim from the provider's backend API.
//!
//! Two tiers on purpose, matching how `clerk_scope_fetch.zig` splits itself:
//! the status mapping and the claim extraction are PURE, so every branch is
//! provable without a listener; the request path gets a loopback server.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use afd_auth::principal::Subject;
use afd_identity::capability::{ClaimSource, ClaimUnavailable};
use afd_identity::provider::{ProviderClaims, UNPROVISIONED_CLAIM, USER_MAX_RESPONSE_BYTES};
use afd_identity::{ProviderClaims as _Reexported, ProviderSecret};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the fake server's request buffer starts at before the read grows it.
const REQUEST_BUFFER_BYTES: usize = 1024;

fn secret() -> ProviderSecret {
    // Deliberately NOT provider-shaped. A realistic `sk_test_…` fixture trips
    // the repository's secret scanner, and the right answer to that is a
    // fixture that cannot be mistaken for a credential — not an allowlist
    // entry, which is how a real leak eventually gets waved through.
    ProviderSecret::new("fixture-backend-secret-not-a-credential").expect("a non-blank secret")
}

fn subject() -> Subject {
    Subject::new("user_2aXyTest").expect("a non-blank subject")
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// Serves one response on a loopback port, recording the request it saw.
async fn serve_once(status: u16, body: Vec<u8>) -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let base = format!("http://{}", listener.local_addr().expect("a bound address"));

    let handle = tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return String::new();
        };
        // Drain the request first: closing a socket with unread bytes makes the
        // kernel send RST, which discards the response tail.
        let mut request = Vec::with_capacity(REQUEST_BUFFER_BYTES);
        let mut byte = [0_u8; 1];
        while socket.read_exact(&mut byte).await.is_ok() {
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let head = format!(
            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(&body).await;
        let _ = socket.shutdown().await;
        String::from_utf8_lossy(&request).into_owned()
    });

    (base, handle)
}

// ── Pure: the status ladder ──────────────────────────────────────────────

/// A 404 says the person is GONE; everything else says we could not find out.
///
/// The distinction the cache acts on: an unknown subject resolves to no
/// capabilities and is NOT cached, while an outage serves a warm entry. Getting
/// these two the wrong way round would either blank a live operator for a
/// freshness window, or tell a terminal to retry a person who no longer exists.
#[test]
fn test_the_status_ladder_separates_gone_from_unreachable() {
    assert_eq!(ProviderClaims::classify(200), None);
    assert_eq!(ProviderClaims::classify(204), None);
    assert_eq!(ProviderClaims::classify(299), None);

    assert_eq!(
        ProviderClaims::classify(404),
        Some(ClaimUnavailable::UnknownSubject)
    );

    for status in [301_u16, 400, 401, 403, 429, 500, 502, 503] {
        assert_eq!(
            ProviderClaims::classify(status),
            Some(ClaimUnavailable::Unreachable),
            "status {status}"
        );
    }
}

/// A blank secret is refused where it is set, not where it is used.
#[test]
fn test_a_blank_provider_secret_is_refused_at_construction() {
    for blank in ["", "   ", "\t\n"] {
        assert!(ProviderSecret::new(blank).is_err(), "{blank:?}");
    }
    // And it never renders, whatever holds it.
    let held = secret();
    let rendered = format!("{held:?}");
    assert!(!rendered.contains("fixture-backend-secret"), "{rendered}");
    assert!(rendered.contains("redacted"), "{rendered}");
}

// ── The request path ─────────────────────────────────────────────────────

/// A provisioned claim comes back, and the request carries the credential.
#[test]
fn test_a_provisioned_claim_is_read_from_public_metadata() {
    support::install_subscriber();
    block_on(async {
        let (base, server) = serve_once(
            200,
            br#"{"id":"user_2aXyTest","public_metadata":{"tenant_id":"t","scopes":"fleet:admin billing:read"}}"#
                .to_vec(),
        )
        .await;

        let claim = ProviderClaims::new(base, secret(), TIMEOUT)
            .expect("a client")
            .claim(&subject())
            .await
            .expect("the provider answered");

        assert_eq!(claim, "fleet:admin billing:read");

        let request = server.await.expect("the server task");
        assert!(
            request.starts_with("GET /users/user_2aXyTest "),
            "the subject keys the lookup: {request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-backend-secret-not-a-credential"),
            "the backend secret authenticates the read"
        );
    });
}

/// Every shape that is not a present string grants nothing.
///
/// `public_metadata` is hand-edited by an operator, so the failure direction
/// matters more than the parse: a malformed metadata object must narrow a
/// principal, never widen one.
#[test]
fn test_every_unusable_metadata_shape_grants_nothing() {
    support::install_subscriber();
    block_on(async {
        for document in [
            // No metadata at all.
            &br#"{"id":"u"}"#[..],
            // Metadata, no scopes key.
            &br#"{"public_metadata":{"tenant_id":"t"}}"#[..],
            // Scopes present, wrong type.
            &br#"{"public_metadata":{"scopes":42}}"#[..],
            &br#"{"public_metadata":{"scopes":null}}"#[..],
            &br#"{"public_metadata":{"scopes":["fleet:admin"]}}"#[..],
            // Metadata itself the wrong type.
            &br#"{"public_metadata":"fleet:admin"}"#[..],
            &br#"{"public_metadata":null}"#[..],
        ] {
            let (base, server) = serve_once(200, document.to_vec()).await;
            let claim = ProviderClaims::new(base, secret(), TIMEOUT)
                .expect("a client")
                .claim(&subject())
                .await
                .expect("a 200 is an answer");
            assert_eq!(
                claim,
                UNPROVISIONED_CLAIM,
                "{}",
                String::from_utf8_lossy(document)
            );
            server.await.expect("the server task");
        }
    });
}

/// A 200 carrying something that is not a user document is not evidence about
/// the person, so it is an outage rather than an empty grant.
#[test]
fn test_a_body_that_is_not_a_user_document_is_an_outage() {
    support::install_subscriber();
    block_on(async {
        for document in [
            &b"not json"[..],
            &b"[]"[..],
            &b"\"a string\""[..],
            &b"42"[..],
        ] {
            let (base, server) = serve_once(200, document.to_vec()).await;
            let refused = ProviderClaims::new(base, secret(), TIMEOUT)
                .expect("a client")
                .claim(&subject())
                .await
                .expect_err("not a user document");
            assert_eq!(
                refused,
                ClaimUnavailable::Unreachable,
                "{}",
                String::from_utf8_lossy(document)
            );
            server.await.expect("the server task");
        }
    });
}

/// A 404 reaches the caller as the person being gone.
#[test]
fn test_a_404_is_the_person_being_gone() {
    support::install_subscriber();
    block_on(async {
        let (base, server) = serve_once(404, b"{}".to_vec()).await;
        let refused = ProviderClaims::new(base, secret(), TIMEOUT)
            .expect("a client")
            .claim(&subject())
            .await
            .expect_err("the provider does not know them");
        assert_eq!(refused, ClaimUnavailable::UnknownSubject);
        server.await.expect("the server task");
    });
}

/// A document past the cap is refused rather than accumulated.
#[test]
fn test_a_user_document_past_the_cap_is_refused() {
    support::install_subscriber();
    block_on(async {
        let (base, server) = serve_once(200, vec![b'x'; USER_MAX_RESPONSE_BYTES + 1]).await;
        let refused = ProviderClaims::new(base, secret(), TIMEOUT)
            .expect("a client")
            .claim(&subject())
            .await
            .expect_err("past the cap");
        assert_eq!(refused, ClaimUnavailable::Unreachable);
        server.await.expect("the server task");
    });
}

/// Nothing listening is an outage, not a person being gone.
#[test]
fn test_an_unreachable_provider_is_an_outage() {
    support::install_subscriber();
    block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let addr = listener.local_addr().expect("a bound address");
        drop(listener);

        let refused = ProviderClaims::new(
            format!("http://{addr}"),
            secret(),
            std::time::Duration::from_millis(500),
        )
        .expect("a client")
        .claim(&subject())
        .await
        .expect_err("nothing is listening");

        assert_eq!(refused, ClaimUnavailable::Unreachable);
    });
}

/// The type is reachable from the crate root, which is how a host wires it.
#[test]
fn test_the_claim_reader_is_exported_from_the_crate_root() {
    let _: fn(String, ProviderSecret, std::time::Duration) -> _ = _Reexported::new;
}
