//! Connecting GitHub: the installation the person reaches is the one bound.
//!
//! The Slack suite beside this one proves the exchange, the seal and the
//! routing row. What GitHub adds is a SECOND vendor conversation after the
//! exchange — which installations of this App can the authorized person
//! reach — and every case here is about that answer and what the daemon does
//! with it. The fake provider serves both calls off one origin, which is what
//! `afd_connector::endpoint` promises a pinned lane.
//!
//! # Every refusal here leaves nothing behind
//!
//! None listed, several listed, a claim the token does not open, and an
//! installation another workspace already routes all answer the ownership
//! code — and after each the vault holds no handle and the routing table is
//! as it was. The assertions say so explicitly, because a refusal that sealed
//! half a grant is the state the exclusive claim exists to prevent.
//!
//! # Every test mints its own installation
//!
//! `core.connector_installs` is keyed on `(provider, external_account_id)`
//! and these tests share one database lane. A literal id shared between them
//! makes the second test to run meet the first one's row as "held elsewhere"
//! — the product working exactly as [`an_installation_another_workspace_routes_is_refused_and_stays_where_it_is`]
//! asks it to, and a fixture collision rather than a finding. Two workspaces
//! claiming ONE installation is a state one test arranges on purpose; every
//! other test needs an id of its own.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use afd_connector::Provider;
use afd_core::error_code;
use http::StatusCode;
use serde_json::Value;

use super::integration_connector_callback::fake_provider::{FakeProvider, Read};
use super::integration_connector_callback::fixture::Fixture;
use super::integration_connector_callback::{complete, complete_with, start_connect};
use crate::harness::json_body;

/// The provider these tests connect.
const PROVIDER: Provider = Provider::GitHub;

/// The user token the vendor issues for the person.
const USER_TOKEN: &str = "gho_fixture_user_token";

/// The account an installation is installed on.
const ACCOUNT: &str = "acme";

/// The vendor path the listing is served at, as the daemon spells it.
const LISTING_PATH: &str = "/user/installations";

/// The handle fields the broker's mint reads back.
const HANDLE_INSTALLATION_ID: &str = "installation_id";
/// See [`HANDLE_INSTALLATION_ID`].
const HANDLE_INTEGRATION: &str = "integration";
/// See [`HANDLE_INSTALLATION_ID`].
const HANDLE_LABEL: &str = "label";

/// The exchange answer: a bearer for the person, nothing more.
const EXCHANGE: &str =
    r#"{"access_token":"gho_fixture_user_token","token_type":"bearer","scope":""}"#;

/// The listing naming none.
const NONE_LISTED: &str = r#"{"total_count":0,"installations":[]}"#;

/// The probe's answer for an installation the token opens.
const PROBE_OPENS: &str = r#"{"total_count":1,"repositories":[{"full_name":"acme/platform"}]}"#;

/// GitHub's answer for an installation the token does not open.
const PROBE_REFUSED: &str = r#"{"message":"Not Found"}"#;
/// What a vendor says when it will not process the request at all, as opposed
/// to processing it and refusing the caller.
const VENDOR_DECLINED: &str = r#"{"message":"Request forbidden by administrative rules."}"#;

/// The vendor's `github-app` bag this deployment connects with.
const APP_BAG: &str =
    r#"{"client_id":"fixture-github-client","client_secret":"fixture-github-secret"}"#;

/// How many installations this suite has minted in this process.
static MINTED_INSTALLATIONS: AtomicU64 = AtomicU64::new(0);

/// An installation id no other test on this lane is using — see the module
/// note on why a shared literal is a collision rather than a finding.
///
/// The clock separates one RUN from the next, and the counter separates the
/// tests inside a run from each other. A minted identifier is deliberately
/// not the source: `mint_id`'s uuids share a long constant prefix, so the
/// digits taken off one are the digits taken off the next, and every test
/// went back to fighting over one id.
fn fresh_installation() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let seq = MINTED_INSTALLATIONS.fetch_add(1, Ordering::Relaxed);
    // Leads with a digit that is not zero, as GitHub's own ids do.
    format!("9{nanos}{seq:04}")
}

/// The listing a person with exactly one installation gets.
fn listing_one(installation: &str) -> Read {
    Read {
        path: LISTING_PATH.to_owned(),
        status: 200,
        body: format!(
            r#"{{"total_count":1,"installations":[{{"id":{installation},"account":{{"login":"{ACCOUNT}","type":"Organization"}}}}]}}"#
        ),
    }
}

/// The listing a person with two installations gets.
fn listing_two(first: &str, second: &str) -> Read {
    Read {
        path: LISTING_PATH.to_owned(),
        status: 200,
        body: format!(
            r#"{{"total_count":2,"installations":[{{"id":{first},"account":{{"login":"{ACCOUNT}"}}}},{{"id":{second},"account":{{"login":"other"}}}}]}}"#
        ),
    }
}

/// The listing when the vendor declines to answer it at all.
///
/// Distinct from every listing above, which are answers: this is GitHub
/// refusing the question. A missing `User-Agent` produced exactly this shape
/// live, and the daemon reported it as a failed token exchange.
fn listing_declined(status: u16) -> Read {
    Read {
        path: LISTING_PATH.to_owned(),
        status,
        body: VENDOR_DECLINED.to_owned(),
    }
}

/// The listing naming nothing.
fn listing_none() -> Read {
    Read {
        path: LISTING_PATH.to_owned(),
        status: 200,
        body: NONE_LISTED.to_owned(),
    }
}

/// The one-repository probe of a claimed installation.
fn probe(installation: &str, status: u16, body: &str) -> Read {
    Read {
        path: format!("{LISTING_PATH}/{installation}/repositories"),
        status,
        body: body.to_owned(),
    }
}

async fn github_fixture() -> Fixture {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal(&PROVIDER.app_key(), APP_BAG).await;
    fixture
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn connect_restores_an_existing_installation() {
    // The drift the whole leg exists for: the App is installed, the datastore
    // was rebuilt, and the workspace has no handle. Pressing Connect lists the
    // one installation the person reaches and binds it — handle sealed under
    // the provider's key, routing row naming the workspace — with no trip
    // through GitHub's install settings.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider =
        FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_one(&installation)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let landed = complete(&router, &fixture, PROVIDER, &state).await;
    let status = landed.status();
    assert_eq!(
        status,
        StatusCode::FOUND,
        "the connect lands: {}",
        json_body(landed).await
    );
    assert_eq!(provider.exchanges(), 1);
    assert_eq!(provider.reads(), 1, "one listing, no probe");

    let grant = fixture
        .grant(PROVIDER)
        .await
        .expect("the connected workspace holds the handle");
    assert_eq!(
        grant.get(HANDLE_INTEGRATION).and_then(Value::as_str),
        Some(PROVIDER.id())
    );
    assert_eq!(
        grant.get(HANDLE_INSTALLATION_ID).and_then(Value::as_str),
        Some(installation.as_str()),
        "the handle names the installation the broker mints from: {grant}"
    );
    assert_eq!(
        grant.get(HANDLE_LABEL).and_then(Value::as_str),
        Some(ACCOUNT),
        "the account is what a person sees the connection called: {grant}"
    );
    assert!(
        !grant.to_string().contains(USER_TOKEN),
        "the person's bearer never lands in the handle: {grant}"
    );
    assert_eq!(
        fixture.routed_to(PROVIDER, &installation).await,
        vec![fixture.workspace.as_str().to_owned()],
        "the App ingress resolves this installation to the workspace"
    );

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_claimed_installation_is_probed_and_bound_only_when_the_token_opens_it() {
    // GitHub's install return carries `installation_id`. It is a CLAIM: the
    // daemon spends the user token on one repository read of it and binds
    // only when that answers, so a person cannot type another organisation's
    // id into the callback URL.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider = FakeProvider::answering_with_reads(
        &[EXCHANGE, EXCHANGE],
        vec![probe(&installation, 200, PROBE_OPENS)],
    )
    .await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let landed = complete_with(
        &router,
        &fixture,
        PROVIDER,
        &state,
        &format!("&installation_id={installation}"),
    )
    .await;
    let status = landed.status();
    assert_eq!(status, StatusCode::FOUND, "{}", json_body(landed).await);
    assert_eq!(provider.reads(), 1, "the claim is probed, not listed");
    let grant = fixture.grant(PROVIDER).await.expect("the claim landed");
    assert_eq!(
        grant.get(HANDLE_INSTALLATION_ID).and_then(Value::as_str),
        Some(installation.as_str())
    );

    // A claim that is not even an id is refused before any store is asked.
    let again = start_connect(&router, &fixture, PROVIDER).await;
    let malformed =
        complete_with(&router, &fixture, PROVIDER, &again, "&installation_id=12a").await;
    let status = malformed.status();
    let document = json_body(malformed).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::INVALID_REQUEST.as_str())
    );
    assert_eq!(provider.exchanges(), 1, "a malformed claim redeems nothing");

    provider.close();
    fixture.cleanup().await;
}

#[path = "integration_connector_github/refusals.rs"]
mod refusals;

#[path = "integration_connector_github/replacement.rs"]
mod replacement;
