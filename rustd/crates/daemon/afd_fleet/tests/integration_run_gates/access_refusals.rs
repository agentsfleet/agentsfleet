//! What a run's access request is refused, and what the approved grant
//! already answers without asking a person.

use super::access::{Live, SENTRY, rows, store};
use super::*;

use af_core::error_code;
use afd_approval::Requested;
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
    store(&fixtures, &live.workspace, SENTRY, &body).await;

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

// Value: protects=a wider GitHub request asks a person once and leaves the
// approved scope in force until the answer; fails_when=the request overwrites
// the approved scope, or raises no card; seam=`RunService::request_access`
// over live Postgres; no_claim=the write mint, which the route step drives.
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

    assert!(matches!(asked, Requested::Raised { .. }), "{asked:?}");
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 1));
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
    fixtures.cleanup().await;
}
