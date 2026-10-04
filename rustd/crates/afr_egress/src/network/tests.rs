#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use reqwest::header::HeaderMap;
use reqwest::{Method, Url};

use super::{BlockedAddress, Capped, Network, guarded_lookup, origin_of};
use crate::error::raise;
use crate::fixture::shown;
use crate::transport::{Outbound, Transport};

#[test]
fn should_keep_a_body_whole_under_the_cap_and_cut_it_past() {
    let mut whole = Capped::default();
    assert!(whole.push(b"abc", 6));
    assert!(whole.push(b"def", 6));
    assert_eq!(
        (whole.bytes.as_slice(), whole.truncated),
        (b"abcdef".as_slice(), false)
    );

    let mut cut = Capped::default();
    assert!(cut.push(b"abcd", 6));
    assert!(!cut.push(b"efgh", 6));
    assert_eq!(
        (cut.bytes.as_slice(), cut.truncated),
        (b"abcdef".as_slice(), true)
    );
}

#[test]
fn should_show_a_redirect_as_its_origin_alone() {
    let request =
        Url::parse("https://api.github.com/repos/acme/widgets/actions/jobs/7/logs").unwrap();

    assert_eq!(
        origin_of(&request, "https://logs.example.net/job/7?sig=secret").as_deref(),
        Some("https://logs.example.net")
    );
    assert_eq!(
        origin_of(&request, "/elsewhere?token=1").as_deref(),
        Some("https://api.github.com")
    );
}

#[tokio::test]
async fn should_refuse_a_name_that_resolves_to_loopback() {
    let refused = guarded_lookup("localhost".to_owned()).await.err();

    assert!(refused.is_some_and(|refusal| refusal.is::<BlockedAddress>()));
}

#[tokio::test]
async fn should_answer_address_not_allowed_when_the_resolver_refuses_the_name() {
    let network = Network::new().unwrap();
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse("https://localhost:9/").unwrap(),
        headers: HeaderMap::new(),
        body: None,
    };

    let refused = network.send(outbound).await.err().as_ref().map(shown);

    assert_eq!(
        refused,
        Some(shown(&raise::address_not_allowed("localhost")))
    );
}

#[tokio::test]
async fn should_answer_unreachable_when_the_name_does_not_resolve() {
    let network = Network::new().unwrap();
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse("https://no-such-host.invalid/").unwrap(),
        headers: HeaderMap::new(),
        body: None,
    };

    let refused = network.send(outbound).await.err().as_ref().map(shown);

    assert_eq!(
        refused,
        Some(shown(&raise::upstream_unreachable(
            "no-such-host.invalid",
            super::NOT_CONNECTED
        )))
    );
}

// A host that accepts and never answers: the handshake waits on a reply that
// never comes, so only the timer ends the request, and the paused clock runs
// it out at once.
#[tokio::test(start_paused = true)]
async fn should_answer_timed_out_when_the_host_never_answers() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _held = listener.accept().await;
        std::future::pending::<()>().await;
    });
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("https://{address}/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
    };

    let refused = Network::new()
        .unwrap()
        .send(outbound)
        .await
        .err()
        .as_ref()
        .map(shown);

    let host = address.ip().to_string();
    let timed_out = raise::upstream_unreachable(&host, super::TIMED_OUT);
    assert_eq!(refused, Some(shown(&timed_out)));
}
