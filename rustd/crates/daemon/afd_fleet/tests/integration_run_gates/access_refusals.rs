//! What a run's access request is refused, and what the approved grant
//! already answers without asking a person.

use super::access::{Live, SENTRY, answer, rows};
use super::*;

use af_core::error_code;
use af_fleet_runtime::config::Access;
use afd_approval::{Decision, Requested};
use serde_json::{Value, json};

use crate::grants::GITHUB;

/// A GitHub scope over `org/app` at `access`.
fn app_scope(access: &str) -> Value {
    if access == "write" {
        json!({"repositories": ["org/app"], "access": "write", "base": "main"})
    } else {
        json!({"repositories": ["org/app"], "access": access})
    }
}

/// How far a GitHub mint under the live run reaches. The mint verb walks
/// past the grant gate to the exchange, which this deployment, holding no App,
/// answers as unconfigured; the binding it narrows the token to is the
/// approved scope it reads at the mint. `None` mints no repository.
async fn mint_reach(fixtures: &Fixtures, live: &Live) -> Option<Access> {
    let run = live.run_id();
    let request = crate::requests::mint(run.as_str(), GITHUB);
    let refused = fixtures
        .plane()
        .mint(&live.ready.runner, &request, fixtures.enrolled_at())
        .await
        .expect_err("no App is configured to exchange with");
    assert_eq!(
        refused.code(),
        afd_fleet::Error::mint_unconfigured().code(),
        "the mint passed the grant gate"
    );
    let scope = fixtures
        .runs()
        .mint_scope(&live.ready.runner, &run)
        .await
        .expect("the mint scope reads")
        .expect("the live run authorises a mint");
    scope.binding.map(|binding| binding.access())
}

/// The code `asked` was refused with.
fn refused(asked: afd_fleet::Result<Requested>) -> Option<error_code::ErrorCode> {
    asked.err().map(|refusal| refusal.code())
}

// Value: protects=a name the workspace's vault does not hold is refused and
// nothing is written; fails_when=a grant or card is written for a name no
// run could ever use; seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_for_a_secret_the_workspace_lacks_is_refused() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;

    let asked = live.ask(&fixtures, SENTRY, None).await;

    assert_eq!(
        refused(asked),
        Some(error_code::FLEET_ACCESS_NAME_NOT_FOUND)
    );
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=an integration the workspace never connected is refused and
// nothing is written; fails_when=the request raises a card for a provider no
// run can mint from; seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_for_an_unconnected_integration_is_refused() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;

    let asked = live.ask(&fixtures, GITHUB, Some(app_scope("read"))).await;

    assert_eq!(
        refused(asked),
        Some(error_code::FLEET_PROVIDER_NOT_CONNECTED)
    );
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=a GitHub request names its repositories and reach, read by
// the one scope parser; fails_when=a request with no scope, or one the parser
// refuses, raises a card; seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_github_request_without_a_scope_is_refused() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    fixtures.connect_github(&live.workspace).await;

    let missing = live.ask(&fixtures, GITHUB, None).await;
    let unreadable = live
        .ask(&fixtures, GITHUB, Some(json!({"repositories": "org/app"})))
        .await;

    assert_eq!(refused(missing), Some(error_code::INVALID_REQUEST));
    assert_eq!(refused(unreadable), Some(error_code::INVALID_REQUEST));
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=a secret whose host field no request could reach cannot be
// asked for; fails_when=the host is put in the egress allow list unread;
// seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_secret_with_an_unreadable_host_cannot_be_requested() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let body = json!({"host": "not a host at all", "token": "fixture"});
    fixtures.store_secret(&live.workspace, SENTRY, &body).await;

    let asked = live.ask(&fixtures, SENTRY, None).await;

    assert_eq!(refused(asked), Some(error_code::INVALID_REQUEST));
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=a request inside the approved GitHub scope is answered at
// once with no card; fails_when=a covered request asks a person again;
// seam=`RunService::request_access` over live Postgres, the grant approved.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_inside_the_approved_scope_needs_no_card() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    fixtures.connect_github(&live.workspace).await;
    fixtures
        .approve_github(&live.ready.fleet, &app_scope("read"))
        .await;

    let asked = live
        .ask(&fixtures, GITHUB, Some(app_scope("read")))
        .await
        .expect("a covered request is answered");

    assert!(matches!(asked, Requested::Covered { .. }), "{asked:?}");
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 0));
    fixtures.cleanup().await;
}

// Value: protects=a wider GitHub request asks a person once, leaves the
// approved read scope in force so a mint reaches no write until Approve, and
// Approve moves the write into the scope the next mint reads; fails_when=the
// request overwrites the approved scope, raises no card, or the mint reads the
// requested scope; seam=`RunService::request_access`, the card's resolve and
// the mint's scope read over live Postgres; no_claim=the vendor exchange,
// which `afd_credential`'s GitHub suite proves narrows to the binding.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_wider_github_request_keeps_the_old_scope_until_approved() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    fixtures.connect_github(&live.workspace).await;
    fixtures
        .approve_github(&live.ready.fleet, &app_scope("read"))
        .await;

    let asked = live
        .ask(&fixtures, GITHUB, Some(app_scope("write")))
        .await
        .expect("a wider request is recorded");

    let gate = match asked {
        Requested::Raised { gate, .. } => Some(gate),
        Requested::Pending { .. } | Requested::Covered { .. } => None,
    }
    .expect("a wider request raises a card");
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 1));
    assert_eq!(
        mint_reach(&fixtures, &live).await,
        Some(Access::Read),
        "a write mint is refused before Approve: the mint reaches read alone"
    );
    let (held, wanted): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT scope ->> 'access', requested_scope ->> 'access' \
           FROM connector.integration_grants WHERE fleet_id = $1 AND service = $2",
    )
    .bind(&live.ready.fleet)
    .bind(GITHUB)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the fleet's GitHub grant");
    assert_eq!(
        (held.as_deref(), wanted.as_deref()),
        (Some("read"), Some("write"))
    );

    answer(&fixtures, &live.ready.fleet, &gate, Decision::Approved).await;
    assert_eq!(
        mint_reach(&fixtures, &live).await,
        Some(Access::Write),
        "after Approve the same run's mint reaches write"
    );
    fixtures.cleanup().await;
}
