//! What happens the SECOND time a callback arrives.
//!
//! The parent proves a first connect: the identity that started it finishes
//! it, the grant seals, the routing row lands or nothing does. These two ask
//! what the same flow does when it has run before — a replayed callback must
//! not spend the code again, and a genuine reconnect must replace the sealed
//! grant rather than refuse it. Both need a connect to already have happened,
//! which is the setup the parent cases do not carry.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_replayed_callback_is_refused_without_redeeming_the_code_again() {
    // The single-use slot, against the Redis that holds it. Without it, anyone
    // who saw a callback URL — a browser history, a proxy log, a referrer —
    // could replay it, and each replay would redeem the code again.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(&fixture, BOT_TOKEN)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    assert_eq!(
        complete(&router, &fixture, PROVIDER, &state).await.status(),
        StatusCode::FOUND
    );

    let replayed = complete(&router, &fixture, PROVIDER, &state).await;
    let status = replayed.status();
    let document = json_body(replayed).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_STATE_INVALID.as_str()),
        "a spent slot answers exactly as a forged state does: both mean start \
         the connect again, and telling them apart is a probe"
    );
    assert_eq!(
        provider.exchanges(),
        1,
        "the replay must not reach the vendor at all — a second redemption \
         would be invisible in the vault, which is why the count is the proof"
    );

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_reconnect_replaces_the_sealed_grant_rather_than_refusing() {
    // A person re-authorising an integration whose token was revoked presses
    // the same button, and the name is already taken. Refusing would leave the
    // dead token in place with no way to replace it but a delete; sealing under
    // a second name would leave a runner opening whichever it found first.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    // One fake answering two tokens in order. A second server would restart
    // the exchange count, and the count is what separates "two connects, one
    // code each" from "one connect that redeemed twice".
    let provider = FakeProvider::answering(&[
        &slack_answer(&fixture, BOT_TOKEN),
        &slack_answer(&fixture, REPLACEMENT_TOKEN),
    ])
    .await;
    let router = fixture.router(&provider);

    let first = start_connect(&router, &fixture, PROVIDER).await;
    assert_eq!(
        complete(&router, &fixture, PROVIDER, &first).await.status(),
        StatusCode::FOUND
    );

    let again = start_connect(&router, &fixture, PROVIDER).await;
    assert_eq!(
        complete(&router, &fixture, PROVIDER, &again).await.status(),
        StatusCode::FOUND,
        "a reconnect is the same action as a connect, not a conflict"
    );
    assert_eq!(
        provider.exchanges(),
        2,
        "each connect redeemed its own code"
    );

    assert_eq!(
        fixture.secret_names().await,
        vec![PROVIDER.grant_key().to_owned()],
        "one name, so a runner cannot open the token that was rotated away"
    );
    assert_eq!(
        fixture
            .grant(PROVIDER)
            .await
            .expect("the workspace still holds a grant")
            .get(HANDLE_BOT_TOKEN)
            .and_then(Value::as_str),
        Some(REPLACEMENT_TOKEN),
        "the standing grant is the newer one"
    );

    provider.close();
    fixture.cleanup().await;
}
