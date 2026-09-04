//! Writing a new account's tenant back to the provider's backend API.
//!
//! The write-side mirror of `provider_claims.rs`, split the same two ways for
//! the same reason: the status ladder is PURE and provable without a listener,
//! and the request path gets a loopback server that records what it was sent.
//!
//! The request assertions are the point. A metadata write that reaches the
//! provider with the wrong verb, the wrong media type or an unmerged body
//! fails silently — `write_signup` swallows its own outcome by design, so
//! nothing downstream would notice until a person's next token carried no
//! `tenant_id` and every call they made was refused for want of a tenant.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::principal::Subject;
use afd_identity::error::MetadataUnwritten;
use afd_identity::{ProviderMetadata, ProviderSecret};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the fake server's request buffer starts at before the read grows it.
const REQUEST_BUFFER_BYTES: usize = 1024;

const TENANT: &str = "tn_01J8XKQ2VN0000000000000000";
const SCOPES: &str = "fleet:admin apikey:admin";

fn secret() -> ProviderSecret {
    // Deliberately NOT provider-shaped, for the reason the read's fixture
    // gives: a realistic `sk_test_…` string trips the repository's secret
    // scanner, and the right answer is a fixture that cannot be mistaken for a
    // credential rather than an allowlist entry.
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

/// Serves one response on a loopback port, recording the whole request.
///
/// Reads to the end of the BODY, not the head: this is the only test file here
/// that asserts on a request body, and stopping at `\r\n\r\n` like the read's
/// helper does would record a head with the merge payload still in the socket.
async fn serve_once(status: u16) -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let base = format!("http://{}", listener.local_addr().expect("a bound address"));

    let handle = tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return String::new();
        };
        let mut request = Vec::with_capacity(REQUEST_BUFFER_BYTES);
        let mut byte = [0_u8; 1];
        // Head first, then exactly the declared body length. Draining fully
        // matters for the same reason it does next door: closing a socket with
        // unread bytes makes the kernel send RST and discards the response.
        while socket.read_exact(&mut byte).await.is_ok() {
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8_lossy(&request).into_owned();
        let declared = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .or_else(|| {
                head.lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0_u8; declared];
        if declared > 0 {
            let _ = socket.read_exact(&mut body).await;
        }
        let response =
            format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
        format!("{head}{}", String::from_utf8_lossy(&body))
    });

    (base, handle)
}

// ── Pure: the status ladder ──────────────────────────────────────────────

/// The three outcomes an operator repairs differently.
///
/// `Unauthorized` is this daemon's own credential being wrong — never
/// transient, never the provider's fault. `UnknownSubject` is the account
/// being gone. Everything else is "we could not find out", which is the only
/// one worth retrying. Collapsing any pair would send an operator to the wrong
/// dashboard.
#[test]
fn test_the_status_ladder_separates_the_three_repairs() {
    assert_eq!(ProviderMetadata::classify(200), None);
    assert_eq!(ProviderMetadata::classify(204), None);
    assert_eq!(ProviderMetadata::classify(299), None);

    assert_eq!(
        ProviderMetadata::classify(401),
        Some(MetadataUnwritten::Unauthorized)
    );
    assert_eq!(
        ProviderMetadata::classify(403),
        Some(MetadataUnwritten::Unauthorized)
    );
    assert_eq!(
        ProviderMetadata::classify(404),
        Some(MetadataUnwritten::UnknownSubject)
    );

    for other in [400_u16, 402, 405, 409, 429, 500, 502, 503] {
        assert_eq!(
            ProviderMetadata::classify(other),
            Some(MetadataUnwritten::Unreachable),
            "{other} should read as unreachable"
        );
    }
}

// ── The request path ─────────────────────────────────────────────────────

/// A successful write sends the merge the provider deep-merges on.
///
/// Every clause here is a silent-failure mode: the wrong verb replaces instead
/// of merging, a missing bearer is a 401 the caller swallows, the wrong media
/// type is a 400, and a body without the `public_metadata` wrapper writes the
/// two keys at the top level where no token template reads them.
#[test]
fn test_a_successful_write_sends_the_merge_the_provider_reads() {
    block_on(async {
        let (base, server) = serve_once(200).await;
        let writer = ProviderMetadata::new(base, secret(), TIMEOUT).expect("a client");

        let outcome = writer.write_signup(&subject(), TENANT, SCOPES).await;
        assert!(outcome.is_ok(), "a 200 is not a refusal: {outcome:?}");

        let seen = server.await.expect("the server task");
        assert!(
            seen.starts_with("PATCH /users/user_2aXyTest/metadata "),
            "wrong verb or path: {seen}"
        );
        assert!(
            seen.to_lowercase()
                .contains("authorization: bearer fixture-backend-secret-not-a-credential"),
            "the daemon's credential did not travel: {seen}"
        );
        assert!(
            seen.to_lowercase()
                .contains("content-type: application/json"),
            "wrong media type: {seen}"
        );
        assert!(
            seen.contains(r#""public_metadata""#),
            "the merge wrapper is missing, so the keys land where nothing reads them: {seen}"
        );
        assert!(seen.contains(TENANT), "the tenant did not travel: {seen}");
        assert!(seen.contains(SCOPES), "the grant did not travel: {seen}");
    });
}

/// A refused credential is named as such, not as an outage.
#[test]
fn test_a_refused_credential_is_not_reported_as_an_outage() {
    for status in [401_u16, 403] {
        block_on(async move {
            let (base, server) = serve_once(status).await;
            let writer = ProviderMetadata::new(base, secret(), TIMEOUT).expect("a client");

            let outcome = writer.write_signup(&subject(), TENANT, SCOPES).await;
            assert_eq!(
                outcome,
                Err(MetadataUnwritten::Unauthorized),
                "{status} should name the credential"
            );
            let _ = server.await;
        });
    }
}

/// A subject the provider does not know is distinguishable from an outage.
#[test]
fn test_an_unknown_subject_is_distinguishable_from_an_outage() {
    block_on(async {
        let (base, server) = serve_once(404).await;
        let writer = ProviderMetadata::new(base, secret(), TIMEOUT).expect("a client");

        assert_eq!(
            writer.write_signup(&subject(), TENANT, SCOPES).await,
            Err(MetadataUnwritten::UnknownSubject)
        );
        let _ = server.await;
    });
}

/// A provider error reads as unreachable — the retryable one.
#[test]
fn test_a_provider_error_reads_as_unreachable() {
    block_on(async {
        let (base, server) = serve_once(500).await;
        let writer = ProviderMetadata::new(base, secret(), TIMEOUT).expect("a client");

        assert_eq!(
            writer.write_signup(&subject(), TENANT, SCOPES).await,
            Err(MetadataUnwritten::Unreachable)
        );
        let _ = server.await;
    });
}

/// A provider that answers nothing at all is unreachable, not a panic.
///
/// The transport arm, which no status can reach: the listener is bound to
/// claim a port and dropped before the request, so the connect is refused. It
/// exists because a signup that panicked here would fail a delivery for an
/// account already committed — the one outcome the module's own header rules
/// out.
#[test]
fn test_a_provider_that_never_answers_is_unreachable_not_a_panic() {
    block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("a bound address"));
        drop(listener);

        let writer = ProviderMetadata::new(base, secret(), TIMEOUT).expect("a client");
        assert_eq!(
            writer.write_signup(&subject(), TENANT, SCOPES).await,
            Err(MetadataUnwritten::Unreachable)
        );
    });
}
