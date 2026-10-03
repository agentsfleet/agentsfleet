#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use reqwest::header::HeaderMap;
use reqwest::{Method, Url};

use super::{BlockedAddress, Capped, Network, guarded_lookup, origin_of};
use crate::refusal::Refusal;
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

    let refused = network.send(outbound).await.err();

    assert_eq!(
        refused,
        Some(Refusal::AddressNotAllowed {
            host: "localhost".to_owned()
        })
    );
}
