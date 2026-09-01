//! The refresh-token connectors' completions — the grant shape Slack is not.
//!
//! `integration_connector_callback.rs` connects Slack because its grant has an
//! INSTALL behind it, and says so in its header. What that choice leaves
//! ungraded is the OTHER shape: the providers whose exchange answers an
//! access/refresh/expiry triple and whose grant exists to be re-minted — the
//! `refresh_triple` parse, Linear's display-name label, and Zoho's data-centre
//! extras. Each arm of `Completions::read` is a provider's own wire dialect,
//! so each gets its own completed connect over the same live fixture.
//!
//! The fake provider answers every exchange whatever the provider, because the
//! lane's pinned endpoint deliberately wins over Zoho's per-callback data
//! centre — the exchange's own doc says a pinned lane must not let a `location`
//! parameter send one provider's exchange to the real vendor.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_connector::Provider;
use http::StatusCode;
use serde_json::Value;

use crate::integration_connector_callback::fake_provider::FakeProvider;
use crate::integration_connector_callback::fixture::Fixture;
use crate::integration_connector_callback::{complete, start_connect};

/// The lifetime every fixture exchange reports, in seconds.
///
/// Named because the seal-time resolution is the claim: the grant stores an
/// INSTANT, so this value and the `expires_at_ms` assertion are one fact, and
/// a literal spelled on both sides could drift on one and still read as a pass.
const EXPIRES_IN_SECONDS: u32 = 3600;

/// The triple every refresh-token provider's exchange answers.
fn triple_answer(extra: &str) -> String {
    format!(
        r#"{{"access_token":"at-fixture-access","refresh_token":"rt-fixture-refresh",
            "expires_in":{EXPIRES_IN_SECONDS}{extra}}}"#
    )
}

/// One completed connect, answered by `body`, with the sealed grant returned.
async fn connected_grant(provider: Provider, body: &str) -> Value {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    // The fixture's own seed registers Slack; this suite's provider needs its
    // OWN app credentials sealed, or the connect refuses `UZ-CONN-001` before
    // any exchange — the operator has not configured that connector.
    fixture
        .seal(
            &provider.app_key(),
            r#"{"client_id":"fixture-client","client_secret":"fixture-secret"}"#,
        )
        .await;
    let vendor = FakeProvider::answering(&[body]).await;
    let router = fixture.router(&vendor);

    let state = start_connect(&router, &fixture, provider).await;
    let landed = complete(&router, &fixture, provider, &state).await;
    assert_eq!(
        landed.status(),
        StatusCode::FOUND,
        "a finished connect sends the person back to the dashboard"
    );
    assert_eq!(vendor.exchanges(), 1);

    let grant = fixture
        .grant(provider)
        .await
        .expect("the connected workspace holds the grant");
    vendor.close();
    fixture.cleanup().await;
    grant
}

/// Linear's completion seals the triple under the provider's display name.
///
/// The label is what the dashboard shows for the connection, and Linear has no
/// per-tenant site to name — the provider's own name is the documented label.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_linear_connect_seals_the_refresh_triple_under_the_display_name() {
    let grant = connected_grant(Provider::Linear, &triple_answer("")).await;

    assert_eq!(grant["integration"].as_str(), Some(Provider::Linear.id()));
    assert_eq!(grant["access_token"].as_str(), Some("at-fixture-access"));
    assert_eq!(
        grant["refresh_token"].as_str(),
        Some("rt-fixture-refresh"),
        "an access token with no refresh half would look connected and stop \
         working within the hour — the triple is the grant"
    );
    assert_eq!(
        grant["label"].as_str(),
        Some(Provider::Linear.display_name())
    );
    assert!(
        grant["expires_at_ms"].as_i64().is_some_and(|at| at > 0),
        "the expiry is resolved to an instant at seal time, not stored as a \
         duration a later reader would have to anchor"
    );
}

/// Zoho's completion carries its data centre, resolved from the callback.
///
/// A later refresh must mint at the same centre the original code was issued
/// by, so the accounts base rides the grant — and with no `location` on the
/// callback, it resolves to the documented default centre rather than failing
/// a connect the vendor considered complete.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_zoho_connect_seals_the_accounts_base_beside_the_triple() {
    let grant = connected_grant(
        Provider::Zoho,
        &triple_answer(r#","api_domain":"https://www.zohoapis.com""#),
    )
    .await;

    assert_eq!(grant["integration"].as_str(), Some(Provider::Zoho.id()));
    assert_eq!(grant["refresh_token"].as_str(), Some("rt-fixture-refresh"));
    let accounts_base = grant["accounts_base"]
        .as_str()
        .expect("a Zoho grant names the centre a refresh mints at");
    assert!(
        accounts_base.starts_with("https://accounts.zoho"),
        "an unnamed location resolves to a documented centre: {accounts_base}"
    );
    assert_eq!(
        grant["label"].as_str(),
        Some("https://www.zohoapis.com"),
        "the vendor's own api_domain labels the connection when it sends one"
    );
}
