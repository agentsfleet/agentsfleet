//! The lifecycle case: create, read, edit, resume, create again, sync and delete,
//! in order, against one fleet.

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, NIGHTLY, create_with, field, schedules};
use crate::harness::{Fleet, json_body, send, send_with_headers};

/// The subject the lifecycle case authenticates as.
///
/// One per TEST rather than one per file, because `core.users.oidc_subject` is
/// globally unique and every case in this suite seeds its own tenant. Two tests sharing
/// a subject is a duplicate-key failure in whichever loses the race, and the
/// other then fails for a reason that has nothing to do with what it asserts.
const SUBJECT_LIFECYCLE: &str = "user_live_fleet_schedules_lifecycle";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_schedules_surface_completes_its_real_lifecycle() {
    let fixture = Fixture::create(SUBJECT_LIFECYCLE).await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_LIFECYCLE,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    a_new_fleet_has_no_schedules(&router, &fixture).await;
    let created = a_create_saves_the_row_and_says_upstream_does_not_know(&router, &fixture).await;
    the_created_schedule_is_readable_and_listed(&router, &fixture, &created).await;
    a_partial_edit_leaves_the_untouched_fields_alone(&router, &fixture, &created).await;
    an_unpause_is_the_same_verb_carrying_the_other_intent(&router, &fixture, &created).await;
    a_second_create_in_the_same_instant_is_a_second_schedule(&router, &fixture, &created).await;
    a_sync_reconciles_the_row_it_names(&router, &fixture, &created).await;
    a_delete_marks_the_row_rather_than_removing_it(&router, &fixture, &created).await;

    fixture.cleanup().await;
}

async fn a_new_fleet_has_no_schedules(router: &axum::Router, fixture: &Fixture) {
    let response = send(
        router,
        Method::GET,
        &fixture.collection(),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        schedules(&document).len(),
        0,
        "a fleet that has registered nothing answers an empty page, not a 404"
    );
}

/// The documented 201-despite-refusal, asserted where it actually happens.
async fn a_create_saves_the_row_and_says_upstream_does_not_know(
    router: &axum::Router,
    fixture: &Fixture,
) -> String {
    let response = send(
        router,
        Method::POST,
        &fixture.collection(),
        Some(&fixture.token),
        &json!({ "cron": NIGHTLY, "message": "run the nightly repair" }).to_string(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the row is saved even though the scheduler could not be reached"
    );
    let document = json_body(response).await;

    assert_eq!(field(&document, "cron"), NIGHTLY);
    assert_eq!(
        field(&document, "timezone"),
        "UTC",
        "a schedule naming no zone is not an error; UTC surprises its author least"
    );
    assert_eq!(
        field(&document, "status"),
        "active",
        "the operator's intent"
    );
    assert_ne!(
        field(&document, "sync"),
        "synced",
        "the scheduler was never reached, and a view claiming otherwise would \
         report a schedule as live when it fires nowhere"
    );

    field(&document, "schedule_id").to_owned()
}

async fn the_created_schedule_is_readable_and_listed(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::GET,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(field(&document, "schedule_id"), schedule);
    assert_eq!(field(&document, "fleet_id"), fixture.fleet.as_str());

    let listed = json_body(
        send(
            router,
            Method::GET,
            &fixture.collection(),
            Some(&fixture.token),
            "",
        )
        .await,
    )
    .await;
    let rows = schedules(&listed);
    assert_eq!(rows.len(), 1);
    let only = rows
        .first()
        .expect("the page carries the one row asserted above");
    assert_eq!(field(only, "schedule_id"), schedule);
}

/// One PATCH carrying the schedule's current version, which an edit requires.
async fn edit(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
    patch: &Value,
) -> axum::response::Response {
    let current = json_body(
        send(
            router,
            Method::GET,
            &fixture.one(schedule),
            Some(&fixture.token),
            "",
        )
        .await,
    )
    .await;
    send_with_headers(
        router,
        Method::PATCH,
        &fixture.one(schedule),
        Some(&fixture.token),
        &patch.to_string(),
        &[(http::header::IF_MATCH, field(&current, "etag"))],
    )
    .await
}

/// An absent field is left alone — a patch is not a whole replacement.
async fn a_partial_edit_leaves_the_untouched_fields_alone(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = edit(router, fixture, schedule, &json!({ "status": "paused" })).await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        field(&document, "status"),
        "paused",
        "the field that was sent"
    );
    assert_eq!(
        field(&document, "cron"),
        NIGHTLY,
        "a patch naming no expression must not blank the one that is there"
    );
    assert_eq!(
        field(&document, "message"),
        "run the nightly repair",
        "nor the message"
    );
}

/// Resuming is `status: active`, and it has to be graded separately.
///
/// The pause case above covers one side of the same choice; this is the other,
/// and they are not symmetric enough to assume. `status` is optional, so the
/// store distinguishes three things a caller can mean — pause, resume, and say
/// nothing about it — and only the third leaves the desired status alone. A
/// resume that fell through to "say nothing" would look correct in the
/// response, which echoes the stored row, and would leave a schedule the
/// operator believes they restarted still paused.
async fn an_unpause_is_the_same_verb_carrying_the_other_intent(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = edit(router, fixture, schedule, &json!({ "status": "active" })).await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        field(&document, "status"),
        "active",
        "`status: active` is a resume; a schedule left paused here is one the \
         operator believes they restarted and did not"
    );
}

/// A second create in the same instant is a second schedule.
///
/// The store names each schedule upstream after its own row id, so two creates
/// inside one millisecond — every create in this test, the suite's clock being
/// frozen — get two keys, and neither can overwrite the other's registration.
async fn a_second_create_in_the_same_instant_is_a_second_schedule(
    router: &axum::Router,
    fixture: &Fixture,
    first: &str,
) {
    let (status, second) = create_with(
        router,
        fixture,
        json!({ "cron": NIGHTLY, "message": "run the nightly repair" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_ne!(
        field(&second, "id"),
        first,
        "a create in the same millisecond is its own row, not the first one rewritten"
    );
}

/// A sync asks for the reconcile the background pass would have made.
///
/// The scheduler is unreachable in this suite, which is the state the module
/// note calls out: the reconcile runs, fails to register, and records that on
/// the row. It still answers 200 with the schedule, because the caller asked
/// for a reconcile and got one — what it produced is in the `sync` field, and
/// a 502 here would tell an operator their schedule is gone when it is not.
async fn a_sync_reconciles_the_row_it_names(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::POST,
        &format!("{}/sync", fixture.one(schedule)),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the reconcile ran; whether it reached the scheduler is the `sync` \
         field's business, not the status code's"
    );
    let document = json_body(response).await;
    assert_ne!(
        field(&document, "sync"),
        "synced",
        "nothing resolves the scheduler here, so a synced verdict would mean \
         the suite is grading a reconcile that did not happen"
    );
}

/// The one that reads as a bug: DELETE leaves the row.
async fn a_delete_marks_the_row_rather_than_removing_it(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::DELETE,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert!(
        response.status().is_success(),
        "the delete is accepted: {}",
        response.status()
    );

    let after = send(
        router,
        Method::GET,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(
        after.status(),
        StatusCode::OK,
        "the row survives until the scheduler confirms — one removed first would \
         leave a schedule firing at a callback that resolves to no fleet"
    );
    assert_eq!(
        field(&json_body(after).await, "status"),
        "deleting",
        "the intent is recorded, and the reconcile is what finishes it"
    );
}
