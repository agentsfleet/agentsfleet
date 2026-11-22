//! A state that verifies but names no workspace this daemon could have
//! signed for.
//!
//! The connect route only ever signs a canonical workspace identifier, so a
//! verified state carrying anything else is this build's fault. It is refused
//! as an invalid state, before the nonce is spent or the vendor is asked,
//! rather than read as a workspace the person lost.

#![cfg(feature = "test-util")]

use super::*;

/// How long the hand-signed state is valid: past any test's run.
const STATE_TTL: std::time::Duration = std::time::Duration::from_mins(10);

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_verified_state_naming_no_workspace_is_refused_as_invalid_before_the_vendor() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(&fixture, BOT_TOKEN)]).await;
    let router = fixture.router(&provider);
    let secret = af_crypto::secret::SecretBytes::new(fixture::STATE_SECRET.as_bytes().to_vec());
    let now = jiff::Timestamp::from_second(harness::frozen_unix_seconds())
        .expect("the frozen instant is a timestamp");
    let state = afd_connector::state::sign(
        PROVIDER.state_binding(),
        &secret,
        "not-a-workspace",
        &fixture.subject,
        "fixture-nonce",
        now,
        STATE_TTL,
    );

    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_STATE_INVALID.as_str()),
        "{document}"
    );
    assert_eq!(provider.exchanges(), 0, "no code is redeemed");
    assert!(fixture.grant(PROVIDER).await.is_none());

    provider.close();
    fixture.cleanup().await;
}
