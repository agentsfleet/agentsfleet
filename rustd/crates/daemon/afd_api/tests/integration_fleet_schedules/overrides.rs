//! A schedule is its own row with no recipe under it, so the route refuses an
//! `override` body and takes the same change sent as the schedule's field.

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, NIGHTLY, create_with, field};
use crate::harness::{Fleet, code_of, json_body, send, send_with_headers};

/// The subject this case authenticates as; one per test, since a subject is
/// globally unique and every case seeds its own tenant.
const SUBJECT_OVERRIDE: &str = "user_live_fleet_schedules_override";

/// The expression the edit moves the schedule to.
const EARLIER: &str = "0 4 * * *";

/// `PATCH` of `schedule` with `body`, under its current tag.
async fn patch(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
    body: &Value,
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
        &body.to_string(),
        &[(http::header::IF_MATCH, field(&current, "etag"))],
    )
    .await
}

/// `PATCH F/schedules/{id}` with an `override` body is 400 `UZ-REQ-001` and
/// changes nothing; with `{cron}` it is 200 and the schedule fires on the new
/// expression.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_schedule_override_is_refused() {
    let fixture = Fixture::create(SUBJECT_OVERRIDE).await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_OVERRIDE,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();
    let (status, created) = create_with(
        &router,
        &fixture,
        json!({ "cron": NIGHTLY, "message": "run the nightly repair" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let schedule = field(&created, "schedule_id").to_owned();

    let refused = patch(
        &router,
        &fixture,
        &schedule,
        &json!({ "override": { "cron": EARLIER } }),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        code_of(refused).await,
        af_core::error_code::INVALID_REQUEST.as_str()
    );
    let unchanged = send(
        &router,
        Method::GET,
        &fixture.one(&schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(field(&json_body(unchanged).await, "cron"), NIGHTLY);

    let taken = patch(&router, &fixture, &schedule, &json!({ "cron": EARLIER })).await;
    assert_eq!(taken.status(), StatusCode::OK);
    assert_eq!(field(&json_body(taken).await, "cron"), EARLIER);
    fixture.cleanup().await;
}
