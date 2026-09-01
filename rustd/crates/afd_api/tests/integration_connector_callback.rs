//! Where a connect actually lands, and what a replay of it does not.
//!
//! `connector_callback_route.rs` proves the browser leg whole and the dashboard
//! leg up to its first vault read, and proves them with no datastore because
//! none of those refusals may reach one. This file starts where that one stops:
//! every case here holds a state this daemon really signed, a nonce really
//! remembered in Redis, and a vendor that really answers.
//!
//! # The whole round trip, not a callback with a hand-made state
//!
//! Each test presses Connect through the real route, takes the state out of the
//! consent URL the daemon composed, and hands that back to the callback. A test
//! that signed its own state would prove the verifier against a fixture rather
//! than against the signer, and the pair could drift into agreeing on a state
//! no provider would ever return.
//!
//! # Single use is proven by the vendor's count, not by the vault
//!
//! A replayed callback that got past the nonce would redeem the code a second
//! time and seal an identical grant. Nothing in the vault distinguishes that
//! from the first connect, so the assertion that carries the property is that
//! the token endpoint was asked exactly once.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_connector::Provider;
use afd_core::error_code;
use http::{Method, StatusCode, header};
use serde_json::Value;

use self::harness::{json_body, send};
#[path = "support/fake_provider.rs"]
pub(crate) mod fake_provider;
#[path = "connector_callback_live/fixture.rs"]
pub(crate) mod fixture;

use self::fake_provider::FakeProvider;
use self::fixture::Fixture;

/// The provider these tests connect.
///
/// Slack rather than one of the refresh-token providers because its grant is
/// the shape with an INSTALL behind it, so a landed connect writes the routing
/// row as well as the sealed handle — the fuller of the two paths.
const PROVIDER: Provider = Provider::Slack;

/// The bot token the vendor issues.
const BOT_TOKEN: &str = "xoxb-fixture-bot-token";

/// The second one, for the reconnect.
const REPLACEMENT_TOKEN: &str = "xoxb-fixture-rotated-token";

/// The team the grant is scoped to.
const TEAM_ID: &str = "T0FIXTURE01";

/// The authorization code the provider hands back.
pub(crate) const CODE: &str = "vendor-authorization-code";

/// The handle field a runner opens the bot token from.
const HANDLE_BOT_TOKEN: &str = "bot_token";

/// The handle field naming which integration it is.
const HANDLE_INTEGRATION: &str = "integration";

/// A token endpoint's answer, in the shape `oauth.v2.access` returns.
fn slack_answer(token: &str) -> String {
    format!(
        r#"{{"ok":true,"access_token":"{token}","bot_user_id":"U0FIXTUREBOT",
            "scope":"chat:write,channels:read",
            "team":{{"id":"{TEAM_ID}","name":"Fixture Workspace"}},
            "authed_user":{{"id":"U0FIXTUREPERSON"}}}}"#
    )
}

/// Presses Connect and answers the state out of the consent URL the daemon
/// composed.
///
/// Taken from the daemon's own URL rather than signed here: the state is what
/// binds workspace, person, nonce and instant together, and a fixture that
/// built one would be asserting the verifier against itself.
pub(crate) async fn start_connect(
    router: &axum::Router,
    fixture: &Fixture,
    provider: Provider,
) -> String {
    let path = format!(
        "/v1/workspaces/{}/connectors/{}/connect",
        fixture.workspace.as_str(),
        provider.id()
    );
    let started = send(router, Method::POST, &path, Some(&fixture.token), "").await;
    let status = started.status();
    let document = json_body(started).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    let consent = document
        .get("install_url")
        .and_then(Value::as_str)
        .expect("a started connect answers a consent URL");
    state_of(consent)
}

/// The `state` parameter of a consent URL, still percent-encoded.
///
/// Left encoded on purpose. The daemon composed this URL through `url`, so the
/// substring is already in the query alphabet, and a provider hands back the
/// same bytes it was given — decoding here only to re-encode differently would
/// be the fixture inventing a spelling no provider sends.
pub(crate) fn state_of(consent: &str) -> String {
    let query = consent
        .split_once('?')
        .expect("a consent URL carries a query")
        .1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("state="))
        .expect("a consent URL carries a state")
        .to_owned()
}

/// The dashboard returning with the person's bearer and the provider's code.
pub(crate) async fn complete(
    router: &axum::Router,
    fixture: &Fixture,
    provider: Provider,
    state: &str,
) -> axum::response::Response {
    let target = format!(
        "/v1/connectors/{}/callback?code={CODE}&state={state}",
        provider.id()
    );
    send(router, Method::POST, &target, Some(&fixture.token), "").await
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_completed_connect_seals_the_grant_under_the_providers_own_key() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(BOT_TOKEN)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let landed = complete(&router, &fixture, PROVIDER, &state).await;
    assert_eq!(
        landed.status(),
        StatusCode::FOUND,
        "a finished connect sends the person back to the dashboard"
    );
    assert!(landed.headers().contains_key(header::LOCATION));

    assert_eq!(
        fixture.secret_names().await,
        vec![PROVIDER.grant_key().to_owned()],
        "the grant is sealed under the provider's own key, which is the name a \
         runner opens it by when a fleet declares the integration"
    );
    let grant = fixture
        .grant(PROVIDER)
        .await
        .expect("the connected workspace holds the grant");
    assert_eq!(
        grant.get(HANDLE_BOT_TOKEN).and_then(Value::as_str),
        Some(BOT_TOKEN),
        "the handle carries what the vendor issued, read out of its own JSON"
    );
    assert_eq!(
        grant.get(HANDLE_INTEGRATION).and_then(Value::as_str),
        Some(PROVIDER.id())
    );
    assert_eq!(provider.exchanges(), 1);

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_replayed_callback_is_refused_without_redeeming_the_code_again() {
    // The single-use slot, against the Redis that holds it. Without it, anyone
    // who saw a callback URL — a browser history, a proxy log, a referrer —
    // could replay it, and each replay would redeem the code again.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(BOT_TOKEN)]).await;
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
    let provider =
        FakeProvider::answering(&[&slack_answer(BOT_TOKEN), &slack_answer(REPLACEMENT_TOKEN)])
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
