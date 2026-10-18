//! The one rendering both schedule surfaces answer through.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just rendered"
)]

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_cron::{DesiredStatus, Reconciled, Refused, Schedule, Source, SyncStatus, validate};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::{checked, held_or, not_fleet_owned, not_found, refused, rendered};

/// What a fixture identifier must be.
const CANONICAL: &str = "a canonical UUIDv7";

/// When the fixture schedule was made and last written.
const MADE_AT: jiff::Timestamp = jiff::Timestamp::constant(1_700_000_000, 0);

/// A schedule a fleet made, as a reconcile hands it back.
fn schedule() -> Schedule {
    Schedule {
        schedule_id: Uuid7::parse("0199a0b0-0000-7000-8000-000000000001").expect(CANONICAL),
        fleet_id: Uuid7::parse("0199a0b0-0000-7000-8000-000000000002").expect(CANONICAL),
        source: Source::Fleet,
        provider_schedule_id: "provider-schedule-1".to_owned(),
        cron: "0 9 * * 1".to_owned(),
        timezone: "Asia/Kolkata".to_owned(),
        message: "weekly check".to_owned(),
        once: true,
        desired_status: DesiredStatus::Active,
        sync_status: SyncStatus::Synced,
        resource_version: 1,
        sync_token: None,
        sync_lease_expires_at: None,
        last_error: None,
        created_at: MADE_AT,
        updated_at: MADE_AT,
        fire_at: None,
        recipe: None,
    }
}

/// A response's body, as JSON.
async fn body_of(response: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is readable");
    serde_json::from_slice(&bytes).expect("the body is JSON")
}

/// A refusal's registry code, read off its problem body.
async fn code_of(refusal: super::Refusal) -> String {
    let body = body_of(refusal.into_response()).await;
    body.get("error_code")
        .and_then(serde_json::Value::as_str)
        .expect("a problem names its code")
        .to_owned()
}

#[tokio::test]
async fn a_reconciled_row_answers_its_view_with_source_and_once() {
    let response = rendered(Reconciled::Synced(schedule()), StatusCode::CREATED, MADE_AT)
        .expect("a synced row renders");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_of(response).await;
    assert_eq!(body["source"], "fleet");
    assert_eq!(body["once"], true);
    assert_eq!(body["sync"], "synced");
}

/// A registered weekly schedule names its next run in RFC 3339, read in its
/// own zone: Monday 09:00 in Kolkata is 03:30 UTC.
#[tokio::test]
async fn a_registered_recurring_schedule_answers_its_next_run_and_words() {
    let weekly = Schedule {
        once: false,
        ..schedule()
    };
    let response = rendered(Reconciled::Synced(weekly), StatusCode::OK, MADE_AT).expect("renders");
    let body = body_of(response).await;
    assert_eq!(body["run_state"], "scheduled");
    assert_eq!(body["next_run_at"], "2023-11-20T03:30:00Z");
    let words = body["description"]
        .as_str()
        .expect("a schedule always carries its description");
    assert!(
        words.ends_with(" (Asia/Kolkata)"),
        "the words name the zone the expression is read in: {words}"
    );
}

/// Saved and not registered is still the row, with its sync state showing.
#[tokio::test]
async fn a_failed_push_still_answers_the_row() {
    let failed = Schedule {
        sync_status: SyncStatus::Failed,
        ..schedule()
    };
    let response = rendered(Reconciled::Failed(failed), StatusCode::OK, MADE_AT).expect("renders");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body["sync"], "failed");
    assert_eq!(
        body["run_state"], "next_run_unavailable",
        "a schedule registered nowhere never shows a run time"
    );
    assert_eq!(body["next_run_at"], serde_json::Value::Null);
}

#[test]
fn a_removed_row_is_a_no_content() {
    let response = rendered(Reconciled::Removed, StatusCode::OK, MADE_AT).expect("renders");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_superseded_reconcile_is_a_conflict_to_retry() {
    let refusal = rendered(Reconciled::Superseded, StatusCode::OK, MADE_AT).expect_err("refused");
    assert_eq!(refusal.status(), StatusCode::CONFLICT);
    let body = body_of(refusal.into_response()).await;
    assert_eq!(body["error_code"], error_code::SCHEDULE_SYNCING.as_str());
    assert_eq!(body["current_state"], super::STATE_SYNCING);
}

#[tokio::test]
async fn no_row_is_not_found() {
    let refusal = held_or(None, StatusCode::OK, MADE_AT).expect_err("refused");
    assert_eq!(refusal.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        code_of(refusal).await,
        error_code::SCHEDULE_NOT_FOUND.as_str()
    );
    assert_eq!(not_found().status(), StatusCode::NOT_FOUND);
    let removed =
        held_or(Some(Reconciled::Removed), StatusCode::OK, MADE_AT).expect("a removed row");
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
}

/// Each refused write answers the code its own table names, and every
/// conflict among them names the state that forbade it.
#[tokio::test]
async fn every_refused_create_answers_its_code() {
    let cases: [(Refused, ErrorCode, StatusCode, Option<&str>); 5] = [
        (
            Refused::NoSuchFleet,
            error_code::SCHEDULE_NOT_FOUND,
            StatusCode::NOT_FOUND,
            None,
        ),
        (
            Refused::TooMany,
            error_code::SCHEDULE_LIMIT_REACHED,
            StatusCode::CONFLICT,
            Some("at_capacity"),
        ),
        (
            Refused::FleetCapReached,
            error_code::SCHEDULE_CAP_REACHED,
            StatusCode::CONFLICT,
            Some("at_capacity"),
        ),
        (
            Refused::DuplicateKey,
            error_code::SCHEDULE_KEY_TAKEN,
            StatusCode::CONFLICT,
            Some("key_held"),
        ),
        (
            Refused::Unheld,
            error_code::RUN_STALE_FENCING_TOKEN,
            StatusCode::CONFLICT,
            Some("superseded"),
        ),
    ];
    for (refusal, code, status, state) in cases {
        let answered = refused(refusal);
        assert_eq!(answered.status(), status, "{refusal:?}");
        let body = body_of(answered.into_response()).await;
        assert_eq!(body["error_code"], code.as_str(), "{refusal:?}");
        assert_eq!(body["current_state"].as_str(), state, "{refusal:?}");
    }
}

#[tokio::test]
async fn a_person_s_schedule_is_forbidden_to_the_fleet() {
    let refusal = not_fleet_owned();
    assert_eq!(refusal.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        code_of(refusal).await,
        error_code::SCHEDULE_NOT_FLEET_OWNED.as_str()
    );
}

/// The validator's refusals reach a caller as a bad request, each with the
/// sentence of the field it broke.
#[tokio::test]
async fn test_schedule_fields_validated() {
    let six_fields = checked(validate::Fields {
        expression: Some("* * * * * *"),
        ..validate::Fields::default()
    })
    .expect_err("a six-field expression is refused");
    let body = body_of(six_fields.into_response()).await;
    assert_eq!(body["error_code"], error_code::INVALID_REQUEST.as_str());
    assert_eq!(body["detail"], validate::DETAIL_INVALID_CRON);

    let unknown_zone = checked(validate::Fields {
        timezone: Some("Mars/Olympus"),
        ..validate::Fields::default()
    })
    .expect_err("an unknown zone is refused");
    assert_eq!(
        body_of(unknown_zone.into_response()).await["detail"],
        validate::DETAIL_INVALID_TIMEZONE
    );

    let over_cap = "m".repeat(validate::MAX_MESSAGE_LEN + 1);
    let too_long = checked(validate::Fields {
        message: Some(&over_cap),
        ..validate::Fields::default()
    })
    .expect_err("a message one byte over the cap is refused");
    assert_eq!(
        body_of(too_long.into_response()).await["detail"],
        validate::DETAIL_MESSAGE_TOO_LONG
    );

    let blank = checked(validate::Fields {
        message: Some(""),
        ..validate::Fields::default()
    })
    .expect_err("an empty message is refused");
    assert_eq!(
        body_of(blank.into_response()).await["detail"],
        validate::DETAIL_INVALID_MESSAGE
    );

    assert!(
        checked(validate::Fields::default()).is_ok(),
        "nothing named, nothing refused"
    );
}
