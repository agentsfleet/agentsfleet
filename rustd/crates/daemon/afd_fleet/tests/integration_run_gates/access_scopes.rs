//! Which access requests take a scope, which handles count as connected, and
//! which approved write scope already covers a request.

use super::access::{Live, SENTRY, rows};
use super::*;

use af_core::error_code;
use afd_approval::Requested;
use serde_json::{Value, json};

use crate::grants::GITHUB;

/// An on-demand integration other than GitHub, which takes no scope.
const LINEAR: &str = "linear";

/// A refresh handle for [`LINEAR`], posting where its connector declares.
fn linear_handle() -> Value {
    json!({"integration": LINEAR, "refresh_token": "fixture-refresh"})
}

/// A write scope over `org/app` that publishes to an existing pull request,
/// validated by one workflow.
fn existing_pr_write() -> Value {
    json!({
        "repositories": ["org/app"],
        "access": "write",
        "base": "main",
        "publication": "existing_pr",
        "validation_workflows": [{
            "path": ".github/workflows/ci.yml",
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        }],
    })
}

/// The code `asked` was refused with.
fn refused(asked: &afd_fleet::Result<Requested>) -> Option<error_code::ErrorCode> {
    asked.as_ref().err().map(afd_fleet::Error::code)
}

// Value: protects=only a GitHub request names a scope, so a scope on a stored
// secret or another integration is refused and nothing is written;
// fails_when=a card is raised carrying a scope no grant of that kind reads;
// seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_scope_on_anything_but_github_is_refused() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let body = json!({"host": "sentry.io", "token": "fixture"});
    fixtures.store_secret(&live.workspace, SENTRY, &body).await;
    fixtures
        .store_secret(&live.workspace, LINEAR, &linear_handle())
        .await;
    let scope = Some(json!({"repositories": ["org/app"], "access": "read"}));

    let secret = live.ask(&fixtures, SENTRY, scope.clone()).await;
    let integration = live.ask(&fixtures, LINEAR, scope).await;

    assert_eq!(refused(&secret), Some(error_code::INVALID_REQUEST));
    assert_eq!(refused(&integration), Some(error_code::INVALID_REQUEST));
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=an on-demand handle no installation owns is connected as it
// stands, and its request asks a person for an integration with no scope;
// fails_when=the request is refused as unconnected, or raises no card;
// seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unowned_integration_handle_raises_one_card() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    fixtures
        .store_secret(&live.workspace, LINEAR, &linear_handle())
        .await;

    let asked = live
        .ask(&fixtures, LINEAR, None)
        .await
        .expect("a stored integration is asked for");

    assert!(matches!(asked, Requested::Raised { .. }), "{asked:?}");
    let (kind, scope): (String, Option<String>) = sqlx::query_as(
        "SELECT kind, scope::text FROM connector.integration_grants \
         WHERE fleet_id = $1 AND service = $2",
    )
    .bind(&live.ready.fleet)
    .bind(LINEAR)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the fleet's grant");
    assert_eq!((kind.as_str(), scope), ("integration", None));
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 1));
    fixtures.cleanup().await;
}

// Value: protects=an on-demand handle filed under a name other than its
// connector's is not a connection, so no card is raised for a connector no
// grant was checked for; fails_when=the misfiled handle is asked for as an
// integration; seam=`RunService::request_access` over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_handle_filed_under_another_name_is_not_connected() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let misfiled = "github-ops";
    let handle = json!({"integration": GITHUB, "installation_id": "4242"});
    fixtures
        .store_secret(&live.workspace, misfiled, &handle)
        .await;

    let asked = live.ask(&fixtures, misfiled, None).await;

    assert_eq!(
        refused(&asked),
        Some(error_code::FLEET_PROVIDER_NOT_CONNECTED)
    );
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (0, 0));
    fixtures.cleanup().await;
}

// Value: protects=a write request inside the approved write scope, its
// publication and every validation workflow held, needs no card;
// fails_when=a covered write asks a person again; seam=`RunService::request_access`
// over live Postgres, the write grant approved.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_write_inside_the_approved_write_scope_needs_no_card() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    fixtures.connect_github(&live.workspace).await;
    fixtures
        .approve_github(&live.ready.fleet, &existing_pr_write())
        .await;

    let asked = live
        .ask(&fixtures, GITHUB, Some(existing_pr_write()))
        .await
        .expect("a covered write is answered");

    assert!(matches!(asked, Requested::Covered { .. }), "{asked:?}");
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 0));
    fixtures.cleanup().await;
}
