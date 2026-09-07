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
    complete_with(router, fixture, provider, state, "").await
}

/// The same completion, with `extra` appended to the callback query — how a
/// provider's return carries more than a code, GitHub's `installation_id`
/// being the one this suite arranges.
pub(crate) async fn complete_with(
    router: &axum::Router,
    fixture: &Fixture,
    provider: Provider,
    state: &str,
    extra: &str,
) -> axum::response::Response {
    complete_as(router, provider, state, &fixture.token, extra).await
}

/// The same completion, presented by whoever holds `token`.
///
/// Takes the token rather than the fixture because the one case that needs it
/// is a person who is NOT the fixture's owner: the live directory resolves a
/// token to its api-key row, so a bystander has to present their own or they
/// are simply the starter again.
pub(crate) async fn complete_as(
    router: &axum::Router,
    provider: Provider,
    state: &str,
    token: &str,
    extra: &str,
) -> axum::response::Response {
    let target = format!(
        "/v1/connectors/{}/callback?code={CODE}&state={state}{extra}",
        provider.id()
    );
    send(router, Method::POST, &target, Some(token), "").await
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_bystander_cannot_finish_somebody_elses_connect_and_the_starter_still_can() {
    // Both halves of the identity binding, in one walk. The refusal alone is
    // proven with no store in `afd_connector/tests/connect_verify.rs`; what
    // only a live run can show is the NON-CONSUMPTION — that the bystander's
    // attempt did not burn the starter's slot, so the starter's own return
    // still lands. A verify that consumed before it compared would pass the
    // first assertion and fail the second.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(BOT_TOKEN)]).await;
    let starter = fixture.router(&provider);
    let bystander = fixture.router_as(&provider, &fixture.bystander);

    let state = start_connect(&starter, &fixture, PROVIDER).await;

    let refused = complete_as(&bystander, PROVIDER, &state, &fixture.bystander_token, "").await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_STATE_INVALID.as_str()),
        "another person's genuine state is refused as invalid, never told apart: {document}"
    );
    assert_eq!(
        provider.exchanges(),
        0,
        "the bystander's attempt must not redeem the code"
    );
    assert!(
        fixture.grant(PROVIDER).await.is_none(),
        "nothing is sealed for a refused completion"
    );

    let landed = complete(&starter, &fixture, PROVIDER, &state).await;
    assert_eq!(
        landed.status(),
        StatusCode::FOUND,
        "the starter's slot survived the bystander: the verify compared before it consumed"
    );
    assert_eq!(provider.exchanges(), 1);
    assert!(fixture.grant(PROVIDER).await.is_some());

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_connect_that_cannot_seal_its_grant_leaves_no_routing_row() {
    // The landing transaction, proven by breaking its second write. The
    // routing row is written first and the grant sealed second, inside one
    // transaction; a vault that refuses the seal must take the row with it,
    // or an inbound delivery would resolve a workspace whose credential is not
    // there. Mutation: commit the row before the seal, and the count below
    // reads one.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let provider = FakeProvider::answering(&[&slack_answer(BOT_TOKEN)]).await;
    let router = fixture.router(&provider);
    let refusing = fixture.refuse_seals().await;

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let failed = complete(&router, &fixture, PROVIDER, &state).await;
    let status = failed.status();
    let document = json_body(failed).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{document}");
    assert_eq!(
        provider.exchanges(),
        1,
        "the code was redeemed — the failure is past the vendor, at the seal"
    );
    assert_eq!(
        fixture.routed_to(PROVIDER, TEAM_ID).await,
        Vec::<String>::new(),
        "the routing row wrote in the same transaction as the seal that failed, \
         and unwound with it"
    );
    assert!(fixture.grant(PROVIDER).await.is_none());

    refusing.lift().await;
    provider.close();
    fixture.cleanup().await;
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

#[path = "integration_connector_callback/second_time.rs"]
mod second_time;
