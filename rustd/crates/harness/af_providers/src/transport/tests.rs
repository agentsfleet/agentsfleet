#![expect(
    clippy::disallowed_methods,
    reason = "the transport under test wraps a plain loopback client; the egress guard and afd_identity::direct are not what these tests prove"
)]

use guffbolt_ai::wire::transport::Send as _;

use super::{guarded, logged};

// A `custom:` endpoint's URL is the tenant's to spell; the retry line is the
// operator's to read, so it carries the host and nothing the URL may hide.
#[test]
fn a_custom_endpoint_is_logged_by_its_host_alone() {
    assert_eq!(
        &*logged("custom:https://user:s3cret@vllm.corp/v1?key=k3y"),
        "custom:vllm.corp"
    );
    assert_eq!(&*logged("anthropic"), "anthropic");
}

// A connection refused by the far side is no guard's refusal: it may pass on
// another send.
#[tokio::test]
async fn a_refused_connection_is_unanswered_and_not_the_guards_refusal() {
    let request = http::Request::builder()
        .uri("http://127.0.0.1:1/")
        .body(bytes::Bytes::new())
        .unwrap_or_default();
    let failure = guarded(reqwest::Client::new()).send(request).await.err();

    assert!(
        failure
            .as_ref()
            .is_some_and(guffbolt_ai::wire::transport::SendError::is_unanswered)
    );
    assert!(
        !failure
            .as_ref()
            .is_some_and(guffbolt_ai::wire::transport::SendError::is_refused)
    );
}
