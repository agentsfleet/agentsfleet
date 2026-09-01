//! What a verified fire does once a row answers for it.
//!
//! `webhook_qstash_route.rs` stops at the last point the handler has acquired
//! nothing: the body cap, a deployment holding no keys, a token that does not
//! verify, and a callback naming no schedule. Everything past that line reads
//! `core.fleet_schedules` joined to `core.fleets`, and over the unreachable
//! pool it can only be a 503 — so the whole tail of the handler read zero
//! covered lines: the three deliberate drops, and the append itself.
//!
//! # The drops are the reason this route is the way it is
//!
//! Each of the three answers 200, and each names a DIFFERENT reason. A route
//! that collapsed them would still pass a status assertion while leaving an
//! operator asking "why did my schedule not run" with nothing — which is what
//! the reason string is for, since the only reader of the body is a scheduler
//! that will never look at it.
//!
//! # Why this suite needs a queue and the others do not
//!
//! The accepted fire is the only path here that WRITES, and what it answers is
//! the identifier of the entry it wrote. `Fleet::live` points the appender at a
//! Redis nothing resolves; `with_live_fire` is the seam that gives it one that
//! takes the append, which is also what makes the retry case reachable at all.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

#[path = "qstash_fire_live/fixture.rs"]
mod fixture;

use self::fixture::Fixture;
use self::harness::json_body;
use crate::webhook_qstash_route::{
    BODY, CURRENT_KEY, FireClaims, HEADER_SCHEDULE, HEADER_SIGNATURE, fire, mint,
};
use afd_cron::DesiredStatus;
use afd_fleet_lifecycle::FleetStatus;
use http::StatusCode;
use serde_json::Value;

/// The reason a fire for a schedule this daemon no longer holds is dropped.
const REASON_NO_SUCH_SCHEDULE: &str = "schedule_not_found";

/// The reason a fire for a schedule nobody wants firing is dropped.
const REASON_SCHEDULE_PAUSED: &str = "schedule_paused";

/// The reason a fire for a fleet an operator halted is dropped.
const REASON_FLEET_PAUSED: &str = "fleet_paused";

/// A schedule identifier no row was ever written for.
const ABSENT_SCHEDULE: &str = "019329c5-0000-7000-8000-0000000000de";

/// A verified fire at `schedule`, signed under this deployment's current key.
///
/// Answers the status as well as the body: every outcome past the wall is a
/// 2xx, and WHICH 2xx is the one distinction between a fire that became work
/// and one that became a log line.
async fn fire_at(router: &axum::Router, schedule: &str, message_id: &str) -> (StatusCode, Value) {
    let token = mint(&FireClaims::for_message(BODY, message_id), CURRENT_KEY);
    let response = fire(
        router,
        BODY,
        &[
            (HEADER_SIGNATURE, token.as_str()),
            (HEADER_SCHEDULE, schedule),
        ],
    )
    .await;

    let status = response.status();
    let document = json_body(response).await;
    assert!(
        status.is_success(),
        "a verified fire is never refused past the wall — a 4xx would put this \
         callback into the scheduler's retry loop, and retrying changes none of \
         these outcomes: {status} {document}"
    );
    (status, document)
}

/// The reason a dropped fire names, or the empty string if it was not dropped.
fn ignored(document: &Value) -> &str {
    document
        .get("ignored")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_fire_for_a_schedule_this_daemon_no_longer_holds_is_dropped() {
    // A real callback: the scheduler was correctly told to send it, and the
    // schedule has since been deleted here. Its own reason rather than the
    // absent-header one, because an operator reading the log needs to know the
    // scheduler is still firing at something this daemon threw away.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Active)
        .await;
    let router = fixture.router();

    let (status, dropped) = fire_at(&router, ABSENT_SCHEDULE, "msg_live_absent_0001").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a drop is an acknowledgement: {dropped}"
    );
    assert_eq!(ignored(&dropped), REASON_NO_SUCH_SCHEDULE, "{dropped}");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_paused_schedules_fire_is_acknowledged_and_dropped() {
    // The scheduler not yet knowing. A pause is written here first and pushed
    // upstream after, so a fire arriving in that window is expected — and
    // answering it 4xx would put a schedule somebody merely paused into the
    // failure rate that gets a whole deployment throttled.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Paused)
        .await;
    let router = fixture.router();

    let (status, dropped) =
        fire_at(&router, fixture.schedule.as_str(), "msg_live_paused_0001").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a drop is an acknowledgement: {dropped}"
    );
    assert_eq!(ignored(&dropped), REASON_SCHEDULE_PAUSED, "{dropped}");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_halted_fleets_fire_is_dropped_for_the_fleets_own_reason() {
    // The other half, and a different fact: the schedule is exactly as its
    // owner wants it, and an operator has stopped everything the fleet does.
    // One reason for both would tell that operator their schedule is paused
    // when it is not, and send them to the wrong surface to fix it.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Stopped, DesiredStatus::Active)
        .await;
    let router = fixture.router();

    let (status, dropped) =
        fire_at(&router, fixture.schedule.as_str(), "msg_live_halted_0001").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a drop is an acknowledgement: {dropped}"
    );
    assert_eq!(ignored(&dropped), REASON_FLEET_PAUSED, "{dropped}");
    assert_ne!(
        REASON_FLEET_PAUSED, REASON_SCHEDULE_PAUSED,
        "the two pauses must read differently, or the assertion above proves \
         only that something was dropped"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_verified_fire_lands_on_the_stream_and_answers_the_event_it_wrote() {
    // The one accepted path, and the only one that writes. 202 rather than 200
    // is the distinction the scheduler cannot see but an operator can: this
    // fire became work, where every 200 above became a log line.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Active)
        .await;
    let router = fixture.router();

    let (status, accepted) =
        fire_at(&router, fixture.schedule.as_str(), "msg_live_accepted_0001").await;

    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "an accepted fire became work, where every drop above became a log \
         line: {accepted}"
    );
    assert_eq!(
        accepted.get("replayed").and_then(Value::as_bool),
        Some(false),
        "the first attempt is not a replay: {accepted}"
    );
    let event = accepted
        .get("event_id")
        .and_then(Value::as_str)
        .expect("an accepted fire names the entry it appended");
    assert!(
        !event.is_empty(),
        "the answer carries the stream's own id, which is what makes a fire \
         traceable to the run it started: {accepted}"
    );
    assert!(
        ignored(&accepted).is_empty(),
        "an accepted fire is not a drop: {accepted}"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_retried_fire_answers_the_first_attempts_event_rather_than_a_second() {
    // The scheduler retries whatever it did not get a 2xx for, repeating its
    // own message id. Without the claim the retry is a second run of the same
    // schedule — the duplicate this route exists to prevent — so the assertion
    // is not just `replayed`, it is that the SAME entry is named back.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Active)
        .await;
    let router = fixture.router();
    let message_id = "msg_live_retried_0001";

    let (_status, first) = fire_at(&router, fixture.schedule.as_str(), message_id).await;
    let (_status, retry) = fire_at(&router, fixture.schedule.as_str(), message_id).await;

    assert_eq!(
        retry.get("replayed").and_then(Value::as_bool),
        Some(true),
        "the second attempt under one message id is a replay: {retry}"
    );
    assert_eq!(
        retry.get("event_id"),
        first.get("event_id"),
        "the retry is answered with the entry the first attempt wrote, not a \
         second one the fleet would run twice"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn two_fires_of_one_schedule_are_two_events() {
    // The other side of the claim key. It is scoped by message id precisely so
    // that a schedule firing twice on two ticks is two runs; a key that was the
    // schedule alone would silence every fire after the first.
    let fixture = Fixture::create().await;
    fixture
        .seed(FleetStatus::Active, DesiredStatus::Active)
        .await;
    let router = fixture.router();

    let (_status, monday) = fire_at(&router, fixture.schedule.as_str(), "msg_live_tick_0001").await;
    let (_status, tuesday) =
        fire_at(&router, fixture.schedule.as_str(), "msg_live_tick_0002").await;

    assert_eq!(
        tuesday.get("replayed").and_then(Value::as_bool),
        Some(false),
        "a different tick is a different fire: {tuesday}"
    );
    assert_ne!(
        monday.get("event_id"),
        tuesday.get("event_id"),
        "two ticks of one schedule are two runs"
    );
    fixture.cleanup().await;
}
