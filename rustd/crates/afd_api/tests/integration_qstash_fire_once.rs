//! A `once` schedule's moment, as the scheduler's callback delivers it: the
//! one fire retires the schedule, and so does a fire dropped because the
//! fleet was halted, since that moment will not come again this year.
//!
//! The fixture's scheduler is one nothing answers, so a retirement here stops
//! at `deleting`: the intent is recorded, and the push upstream is the
//! reconcile's to retry. What matters is that the row no longer fires.

#![cfg(feature = "test-util")]

use afd_cron::DesiredStatus;
use afd_fleet_lifecycle::FleetStatus;
use http::StatusCode;

use crate::integration_qstash_fire::fixture::Fixture;
use crate::webhook_qstash_route::{
    BODY, CURRENT_KEY, FireClaims, HEADER_SCHEDULE, HEADER_SIGNATURE, fire, mint,
};

/// A verified fire at the fixture's schedule, answered status.
async fn fire_once(fixture: &Fixture, message_id: &str) -> StatusCode {
    let token = mint(&FireClaims::for_message(BODY, message_id), CURRENT_KEY);
    fire(
        &fixture.router(),
        BODY,
        &[
            (HEADER_SIGNATURE, token.as_str()),
            (HEADER_SCHEDULE, fixture.schedule.as_str()),
        ],
    )
    .await
    .status()
}

/// Dimension 1.9, from `QStash`. The fire is work, and the schedule retires.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_once_schedule_retires_after_its_qstash_fire() {
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Active)
        .await;
    fixture.mark_once().await;

    assert_eq!(
        fire_once(&fixture, "msg_once_fired_0001").await,
        StatusCode::ACCEPTED
    );
    let left = fixture.desired_status().await;
    assert!(
        left.as_deref().is_none_or(|status| status == "deleting"),
        "a fired one-off no longer fires: {left:?}"
    );
    fixture.cleanup().await;
}

/// A halted fleet drops the fire, and the one-off retires anyway: kept, it
/// would match the same minute next year.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_once_schedule_whose_fire_is_dropped_still_retires() {
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Paused, DesiredStatus::Active)
        .await;
    fixture.mark_once().await;

    assert_eq!(
        fire_once(&fixture, "msg_once_dropped_0001").await,
        StatusCode::OK
    );
    let left = fixture.desired_status().await;
    assert!(
        left.as_deref().is_none_or(|status| status == "deleting"),
        "a missed one-off is retired, not kept for next year: {left:?}"
    );
    fixture.cleanup().await;
}

/// A recurring schedule's dropped fire changes nothing: only a one-off has a
/// moment that passes.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_recurring_schedule_whose_fire_is_dropped_is_kept() {
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Paused, DesiredStatus::Active)
        .await;

    assert_eq!(
        fire_once(&fixture, "msg_kept_dropped_0001").await,
        StatusCode::OK
    );
    assert_eq!(fixture.desired_status().await.as_deref(), Some("active"));
    fixture.cleanup().await;
}
