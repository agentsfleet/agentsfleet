//! A fleet's delete removes its schedules from the scheduler before the fleet,
//! and stops while one is still registered.
//!
//! The scheduler is unreachable here (see the suite note), so these cases
//! stand at the refusal: the removal that cannot happen stops the delete, and
//! a delete refused for its tag touches no schedule. The removal that
//! succeeds is `afd_cron`'s `integration_release_fleet`, against a stub
//! scheduler.

use af_core::error_code;
use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, NIGHTLY, create_with, field};
use crate::harness::{Fleet, json_body, send, send_with_headers};

/// The subjects these cases authenticate as, one per test: a subject is
/// globally unique and every case seeds its own tenant.
const SUBJECT_HELD: &str = "user_live_fleet_delete_schedule_held";
const SUBJECT_STALE: &str = "user_live_fleet_delete_stale_tag";
const SUBJECT_EMPTY: &str = "user_live_fleet_delete_no_schedules";

/// A router over the fixture's live stores, scoped to delete fleets and
/// write their schedules.
fn router(fixture: &Fixture, subject: &str) -> axum::Router {
    Fleet::live(
        fixture.database.clone(),
        subject,
        ScopeSet::from_scopes(&[
            Scope::FleetRead,
            Scope::FleetAdmin,
            Scope::ScheduleRead,
            Scope::ScheduleWrite,
        ]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router()
}

/// The fleet's own path.
fn fleet_path(fixture: &Fixture) -> String {
    format!(
        "/v1/workspaces/{}/fleets/{}",
        fixture.workspace.as_str(),
        fixture.fleet.as_str()
    )
}

/// The fleet's current tag, from its read.
async fn fleet_tag(router: &axum::Router, fixture: &Fixture) -> String {
    let read = send(
        router,
        Method::GET,
        &fleet_path(fixture),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK, "the fleet is readable");
    read.headers()
        .get(http::header::ETAG)
        .and_then(|tag| tag.to_str().ok())
        .expect("a fleet read carries its tag")
        .to_owned()
}

/// `DELETE` of the fleet under `tag`, as its status and document.
async fn delete_fleet(router: &axum::Router, fixture: &Fixture, tag: &str) -> (StatusCode, Value) {
    let response = send_with_headers(
        router,
        Method::DELETE,
        &fleet_path(fixture),
        Some(&fixture.token),
        "",
        &[(http::header::IF_MATCH, tag)],
    )
    .await;
    let status = response.status();
    // A `204` carries no body, which reads as `null`.
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test response body is small and in memory");
    let document = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a refusal is JSON")
    };
    (status, document)
}

/// One schedule's document, read back through the surface.
async fn schedule(router: &axum::Router, fixture: &Fixture, id: &str) -> Value {
    let read = send(
        router,
        Method::GET,
        &fixture.one(id),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK, "the schedule survives");
    json_body(read).await
}

/// A schedule on the fixture's fleet, saved and not registered upstream.
async fn nightly(router: &axum::Router, fixture: &Fixture) -> String {
    let (status, created) = create_with(
        router,
        fixture,
        json!({ "cron": NIGHTLY, "message": "run the nightly repair" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    field(&created, "schedule_id").to_owned()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_delete_stops_while_a_schedule_is_still_registered() {
    let fixture = Fixture::create(SUBJECT_HELD).await;
    fixture.seed().await;
    let router = router(&fixture, SUBJECT_HELD);
    let id = nightly(&router, &fixture).await;
    let tag = fleet_tag(&router, &fixture).await;

    let (status, refused) = delete_fleet(&router, &fixture, &tag).await;

    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        field(&refused, "error_code"),
        error_code::SCHEDULE_SYNCING.as_str()
    );
    assert_eq!(field(&refused, "current_state"), "syncing");
    assert_eq!(
        field(&refused, "detail"),
        "This fleet's schedules are still registered with the scheduler, so the fleet was \
         not deleted. Repeat the delete in a moment."
    );
    assert_eq!(
        fleet_tag(&router, &fixture).await,
        tag,
        "the fleet survives unchanged, so the repeated delete names the same tag"
    );
    let kept = schedule(&router, &fixture, &id).await;
    assert_eq!(
        field(&kept, "status"),
        "deleting",
        "the removal is recorded, and a repeated delete finishes it: {kept}"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stale_fleet_tag_refuses_before_any_schedule_is_removed() {
    let fixture = Fixture::create(SUBJECT_STALE).await;
    fixture.seed().await;
    let router = router(&fixture, SUBJECT_STALE);
    let id = nightly(&router, &fixture).await;
    let tag = fleet_tag(&router, &fixture).await;
    let stale = af_core::etag::versioned(&fixture.fleet, i64::MAX);
    assert_ne!(stale, tag, "the case needs a tag that is not current");

    let (status, refused) = delete_fleet(&router, &fixture, &stale).await;

    assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{refused}");
    assert_eq!(
        field(&refused, "error_code"),
        error_code::SETUP_STALE.as_str()
    );
    let kept = schedule(&router, &fixture, &id).await;
    assert_eq!(
        field(&kept, "status"),
        "active",
        "a delete refused for its tag leaves the schedule's intent alone: {kept}"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_with_no_schedules_deletes_without_asking_the_scheduler() {
    let fixture = Fixture::create(SUBJECT_EMPTY).await;
    fixture.seed().await;
    let router = router(&fixture, SUBJECT_EMPTY);
    let tag = fleet_tag(&router, &fixture).await;

    let (status, body) = delete_fleet(&router, &fixture, &tag).await;

    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "nothing is registered, so the unreachable scheduler is never needed: {body}"
    );
    let gone = send(
        &router,
        Method::GET,
        &fleet_path(&fixture),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND, "the fleet is gone");
    fixture.cleanup().await;
}
