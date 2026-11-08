//! The refusal case: every body, zone, expression and id the surface turns
//! away, and the proof that none of them wrote a row.

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::mint_id;
use http::{Method, StatusCode};
use serde_json::json;

use super::{Fixture, NIGHTLY, create_with, schedules};
use crate::harness::{Fleet, json_body, send};

/// The subject the refusal case authenticates as.
const SUBJECT_REFUSALS: &str = "user_live_fleet_schedules_refusals";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_schedules_surface_refuses_what_it_will_not_register() {
    let fixture = Fixture::create(SUBJECT_REFUSALS).await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_REFUSALS,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    an_expression_this_daemon_will_not_register_is_refused(&router, &fixture).await;
    a_zone_this_daemon_will_not_pass_upstream_is_refused(&router, &fixture).await;
    a_message_that_would_wake_a_fleet_with_nothing_is_refused(&router, &fixture).await;
    a_body_that_is_not_a_schedule_is_refused(&router, &fixture).await;
    a_schedule_this_fleet_does_not_hold_is_not_found(&router, &fixture).await;
    an_edit_is_validated_before_the_row_is_looked_up(&router, &fixture).await;
    a_sync_of_a_schedule_this_fleet_does_not_hold_is_not_found(&router, &fixture).await;
    a_schedule_for_a_fleet_that_does_not_exist_is_refused(&router, &fixture).await;
    nothing_was_written_by_any_refusal(&router, &fixture).await;

    fixture.cleanup().await;
}

async fn an_expression_this_daemon_will_not_register_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for expression in ["@daily", "*/61 * * * *", "5-2 * * * *", "MON * * * *", ""] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": expression,
                "message": "run the nightly repair",
            }),
        )
        .await;
        assert_eq!(
            refused.0,
            StatusCode::BAD_REQUEST,
            "`{expression}` must be refused before a row is written, because a \
             failed registration is a state somebody has to clear"
        );
    }
}

async fn a_zone_this_daemon_will_not_pass_upstream_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for zone in ["Mars/Olympus", "../../etc/passwd", ""] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": NIGHTLY,
                "timezone": zone,
                "message": "run the nightly repair",
            }),
        )
        .await;
        assert_eq!(refused.0, StatusCode::BAD_REQUEST, "zone `{zone}`");
    }
}

async fn a_message_that_would_wake_a_fleet_with_nothing_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for message in ["", "   ", "\n\t "] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": NIGHTLY,
                "message": message,
            }),
        )
        .await;
        assert_eq!(
            refused.0,
            StatusCode::BAD_REQUEST,
            "a fire with nothing to do wakes a fleet to no purpose"
        );
    }
}

async fn a_body_that_is_not_a_schedule_is_refused(router: &axum::Router, fixture: &Fixture) {
    for body in ["", "{", "[]", r#"{"message":"no cron"}"#] {
        let response = send(
            router,
            Method::POST,
            &fixture.collection(),
            Some(&fixture.token),
            body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "body `{body}` is not a schedule this daemon can read"
        );
    }
}

async fn a_schedule_this_fleet_does_not_hold_is_not_found(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent = mint_id();
    let response = send(
        router,
        Method::GET,
        &fixture.one(&absent),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn nothing_was_written_by_any_refusal(router: &axum::Router, fixture: &Fixture) {
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

    assert_eq!(
        schedules(&listed).len(),
        0,
        "every case above was refused before the write, so this fleet still \
         holds nothing"
    );
}

/// Every field `PATCH` accepts is validated, and validated BEFORE the lookup.
///
/// The ordering is the point, not an incidental. `patch` checks the fields it
/// was given and only then asks the store for the row, so an edit carrying an
/// expression the scheduler would reject is refused whether or not the schedule
/// exists — which is why this case can assert all three refusals against a
/// schedule id that was never created, and why the fleet still holds nothing
/// afterwards. Were the lookup to move first, these would answer "not found"
/// and the validation they exist to grade would never run.
async fn an_edit_is_validated_before_the_row_is_looked_up(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent = mint_id();

    for (field, body) in [
        ("cron", json!({ "cron": "not an expression" })),
        ("timezone", json!({ "timezone": "Mars/Olympus" })),
        ("message", json!({ "message": "" })),
    ] {
        let response = send(
            router,
            Method::PATCH,
            &fixture.one(&absent),
            Some(&fixture.token),
            &body.to_string(),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a patch carrying an unusable `{field}` is refused for THAT reason, \
             not as a missing row — the validation runs before the lookup"
        );
    }
}

/// A sync names a schedule, and a name matching nothing is a 404.
///
/// `sync` is the one schedule verb with no body to get wrong, so the only
/// refusal it owns is this one: the reconcile answers `None` for a row this
/// fleet does not hold, and `held_or` turns that into a not-found rather than
/// letting an absent row read as a successful no-op.
async fn a_sync_of_a_schedule_this_fleet_does_not_hold_is_not_found(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let response = send(
        router,
        Method::POST,
        &format!("{}/sync", fixture.one(&mint_id())),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a sync of a schedule this fleet does not hold is a 404, not a silent OK"
    );
}

/// A create names a fleet, and the store is what knows the fleet is real.
///
/// The workspace here is owned and the request is authorised, so nothing before
/// the store has cause to refuse: the path parses, the body validates, and the
/// insert is the first thing that consults `fleets`. What it answers is a
/// refusal and not a database error, because a schedule for a fleet nobody
/// created is a caller mistake rather than an incident — and rendering it as a
/// 500 would page somebody for a typo.
async fn a_schedule_for_a_fleet_that_does_not_exist_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent_fleet = mint_id();
    let response = send(
        router,
        Method::POST,
        &format!(
            "/v1/workspaces/{}/fleets/{absent_fleet}/schedules",
            fixture.workspace.as_str()
        ),
        Some(&fixture.token),
        &json!({ "cron": NIGHTLY, "message": "run the nightly repair" }).to_string(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a schedule for a fleet that was never created is a 404, not a 500"
    );
}
