//! The per-fleet ceiling case: a fleet holding `MAX_SCHEDULES_PER_FLEET`
//! refuses one more, and the refusal writes nothing.

use afd_auth::scope::{Scope, ScopeSet};
use afd_cron::MAX_SCHEDULES_PER_FLEET;
use http::{Method, StatusCode};
use serde_json::json;

use super::{Fixture, NIGHTLY, create_with, schedules};
use crate::harness::{Fleet, json_body, send};

/// The subject the ceiling case authenticates as.
const SUBJECT_CEILING: &str = "user_live_fleet_schedules_ceiling";

/// The per-fleet ceiling is refused at the store, before the row is written.
///
/// `MAX_SCHEDULES_PER_FLEET` bounds the fan-out of a single fleet, and the
/// count is taken inside the same statement that would insert — so the refusal
/// cannot race a concurrent create past the limit. The ceiling is seeded here
/// rather than driven through the API because the suite's clock is frozen: a
/// second create over HTTP collides on the upstream key long before it could
/// reach the thirty-third, which is the duplicate case, not this one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_already_holding_the_ceiling_refuses_one_more() {
    let fixture = Fixture::create(SUBJECT_CEILING).await;
    fixture.seed().await;
    fixture.fill_to_the_ceiling().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_CEILING,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    let refused = create_with(
        &router,
        &fixture,
        json!({ "cron": NIGHTLY, "message": "one too many" }),
    )
    .await;

    assert_eq!(
        refused.0,
        StatusCode::CONFLICT,
        "a fleet at its ceiling refuses the next schedule rather than growing \
         a fan-out nothing bounds"
    );

    let listed = json_body(
        send(
            &router,
            Method::GET,
            &fixture.collection(),
            Some(&fixture.token),
            "",
        )
        .await,
    )
    .await;
    assert_eq!(
        schedules(&listed).len(),
        MAX_SCHEDULES_PER_FLEET,
        "the refused create wrote nothing: the fleet still holds exactly the \
         ceiling it held before"
    );

    fixture.cleanup().await;
}
