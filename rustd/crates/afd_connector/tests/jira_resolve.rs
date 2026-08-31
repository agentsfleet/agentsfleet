//! Jira's second call, over a socket rather than a string comparison.
//!
//! `endpoint::redirected` is unit-tested as composition, which proves the URL
//! this daemon would build and nothing about the request it then makes. The
//! part that matters is downstream of that: whether the GET lands on the
//! vendor's own path at the pinned host, and whether the freshly minted bearer
//! rides with it.
//!
//! # The failure this exists to catch
//!
//! A pin that is not a usable origin must REFUSE. Falling back to the vendor
//! there is the one outcome pinning exists to prevent — a lane sets the knob to
//! keep a test off Atlassian, so a typo in it would send a live access token to
//! Atlassian from CI. That case cannot be observed from a string comparison:
//! only a test that watches whether anything is dialled can tell a refusal from
//! a request that went somewhere else.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_connector::jira;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// The access token the exchange just minted.
const ACCESS_TOKEN: &str = "fixture-jira-access-token";

/// The path Atlassian serves its site listing on — the vendor's, not a lane's.
const VENDOR_PATH: &str = "/oauth/token/accessible-resources";

/// One site, as Atlassian lists it.
const SITES: &str = r#"[{"id":"cloud-1","name":"Acme Jira","url":"https://acme.atlassian.net"}]"#;

/// A loopback Atlassian that records the one request it is given.
struct FakeAtlassian {
    base: String,
    dialled: Arc<AtomicUsize>,
    request: tokio::task::JoinHandle<String>,
}

impl FakeAtlassian {
    async fn listing() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let port = listener.local_addr().expect("the listener is bound").port();
        let dialled = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&dialled);
        let request = tokio::spawn(async move {
            // Bounded: a refusal means nothing connects, and this task has to
            // end for the test to be able to assert that.
            let accepted =
                tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept()).await;
            let Ok(Ok((mut socket, _peer))) = accepted else {
                return String::new();
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = vec![0u8; 4096];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let received =
                String::from_utf8_lossy(buffer.get(..read).unwrap_or_default()).into_owned();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{SITES}",
                SITES.len()
            );
            let _written = socket.write_all(response.as_bytes()).await;
            let _flushed = socket.flush().await;
            received
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            dialled,
            request,
        }
    }

    fn was_dialled(&self) -> bool {
        self.dialled.load(Ordering::SeqCst) > 0
    }

    async fn received(self) -> String {
        self.request.await.expect("the fake Atlassian completed")
    }
}

#[tokio::test]
async fn a_pinned_lane_asks_the_vendors_own_question_at_its_own_host() {
    // The composition, proved by what arrives rather than by what a format
    // string produced: the HOST is the lane's and the PATH is Atlassian's, so a
    // fake serving the real route answers the question the daemon actually asks
    // in production.
    let atlassian = FakeAtlassian::listing().await;
    let site = jira::resolve(
        &reqwest::Client::new(),
        Some(&format!("{}/oauth/v2/token", atlassian.base)),
        ACCESS_TOKEN,
    )
    .await
    .expect("a listing carrying one site resolves");

    assert_eq!(site.cloud_id, "cloud-1");
    assert_eq!(site.name, "Acme Jira");

    let sent = atlassian.received().await;
    assert!(
        sent.starts_with(&format!("GET {VENDOR_PATH}")),
        "the pin moves the host and must not move the path: {sent}"
    );
    assert!(
        sent.to_ascii_lowercase()
            .contains(&format!("authorization: bearer {ACCESS_TOKEN}")),
        "the listing is scoped to the token that was just minted: {sent}"
    );
}

#[tokio::test]
async fn a_pin_that_is_not_a_usable_origin_dials_nothing_at_all() {
    // The finding this file exists for. An empty host composes a URL whose
    // destination the resolver picks, and userinfo reads as the vendor while
    // resolving elsewhere — both used to fall through to Atlassian carrying a
    // live bearer. The assertion is the NEGATIVE one: nothing was dialled.
    for pinned in [
        "http:///127.0.0.1:9931/token",
        "http://vendor.example@evil.test/token",
        "127.0.0.1:9931",
        "http://",
    ] {
        let atlassian = FakeAtlassian::listing().await;
        let refused = jira::resolve(&reqwest::Client::new(), Some(pinned), ACCESS_TOKEN).await;

        assert!(
            refused.is_err(),
            "`{pinned}` must refuse rather than resolve a site"
        );
        assert!(
            !atlassian.was_dialled(),
            "`{pinned}` must reach no host at all — falling back to the vendor \
             would send a live bearer to Atlassian from a lane that pinned \
             precisely to avoid that"
        );
        drop(atlassian);
    }
}

#[tokio::test]
async fn an_unpinned_call_targets_atlassians_own_endpoint() {
    // Nothing is dialled here because `.test` resolves nowhere and the real
    // vendor is not reachable from a test; what is asserted is that the call
    // was ATTEMPTED against a host this fixture never stood up, which is what
    // "no pin means the vendor" has to mean.
    let refused = jira::resolve(&reqwest::Client::new(), None, ACCESS_TOKEN).await;
    assert!(
        refused.is_err(),
        "an unpinned call reaches for Atlassian, which no test can answer"
    );
}
