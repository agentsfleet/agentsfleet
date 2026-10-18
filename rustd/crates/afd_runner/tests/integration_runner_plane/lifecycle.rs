//! Enrolment and the beats after it: what each writes to history, and when the
//! database's clock is read.

use std::time::Duration;

use afd_api_wire::admin::RunnerEventType;
use afd_core::clock;
use afd_core::timing::RUNNER_OFFLINE_AFTER;
use afd_runner::NO_REPORT;
use afd_runner::sql::NEVER_SEEN;
use serde_json::Value;
use sqlx::Connection as _;

use super::Fixture;

/// The metadata key a transition event records the previous beat under.
const LAST_SEEN_AT: &str = "last_seen_at";

/// How long a competing statement holds the runner row.
const LOCK_HELD_FOR: Duration = Duration::from_millis(600);

/// How long the seeded beat stays fresh after the heartbeat asks: well inside
/// the lock hold, so it goes stale while the beat waits.
const FRESH_FOR: Duration = Duration::from_millis(200);

/// Backdates the runner's last beat to `ago` before the database's clock, and
/// answers the instant stored.
const BACKDATE_LAST_SEEN: &str = "\
UPDATE runner.runners SET last_seen_at = statement_timestamp() - $2::interval
WHERE id = $1::uuid RETURNING last_seen_at";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_enrolment_and_first_beat_each_write_one_event() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;

    let enrolled = fixture
        .runners
        .self_record(&runner)
        .await
        .expect("the enrolled row reads");
    assert_eq!(enrolled.status, "active");
    assert_eq!(enrolled.last_seen_at, NEVER_SEEN);
    let history = fixture.history(&runner).await;
    let types: Vec<_> = history.iter().map(|event| event.event_type).collect();
    assert_eq!(types, [RunnerEventType::RunnerRegistered]);

    for _beat in 0..2 {
        fixture
            .runners
            .heartbeat(&runner, &NO_REPORT, None, clock::now())
            .await
            .expect("the beat lands");
    }

    let beaten = fixture
        .runners
        .self_record(&runner)
        .await
        .expect("the beaten row reads");
    assert!(beaten.last_seen_at > NEVER_SEEN, "the beat moved liveness");
    let history = fixture.history(&runner).await;
    let types: Vec<_> = history.iter().map(|event| event.event_type).collect();
    assert_eq!(
        types,
        [
            RunnerEventType::RunnerOnline,
            RunnerEventType::RunnerRegistered
        ],
        "only the first beat is a transition"
    );
    // The previous beat reaches the operator API as RFC 3339, in the form
    // jiff writes: the never-seen sentinel is the epoch.
    let online = history.first().expect("the transition is newest");
    assert_eq!(
        online.metadata.get(LAST_SEEN_AT),
        Some(&Value::String(NEVER_SEEN.to_string()))
    );
    fixture.cleanup().await;
}

/// Dimension 4.8 — a beat that waited on the runner row past the offline
/// threshold is judged at the instant it got the row: the runner had gone
/// stale while it waited, so the beat records the transition.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_beat_that_waited_on_the_lock_past_the_threshold_records_the_transition() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let mut holder = fixture
        .database
        .acquire()
        .await
        .expect("a competing connection");
    let seen: jiff_sqlx::Timestamp = sqlx::query_scalar(BACKDATE_LAST_SEEN)
        .bind(runner.as_str())
        .bind(RUNNER_OFFLINE_AFTER.saturating_sub(FRESH_FOR))
        .fetch_one(&mut *holder)
        .await
        .expect("the last beat backdates");

    let mut held = holder.begin().await.expect("a competing transaction");
    sqlx::query("SELECT 1 FROM runner.runners WHERE id = $1::uuid FOR UPDATE")
        .bind(runner.as_str())
        .execute(&mut *held)
        .await
        .expect("the competing statement holds the row");
    let release = async move {
        tokio::time::sleep(LOCK_HELD_FOR).await;
        held.rollback()
            .await
            .expect("the competing statement lets go");
    };
    let nothing = NO_REPORT;
    let beat = fixture
        .runners
        .heartbeat(&runner, &nothing, None, clock::now());
    let (beat, ()) = tokio::join!(beat, release);
    beat.expect("the beat lands once the row is free");
    drop(holder);

    let history = fixture.history(&runner).await;
    let online = history
        .iter()
        .find(|event| event.event_type == RunnerEventType::RunnerOnline)
        .expect("the beat that waited past the threshold records the transition");
    assert_eq!(
        online.metadata.get(LAST_SEEN_AT),
        Some(&Value::String(seen.to_jiff().to_string()))
    );
    fixture.cleanup().await;
}
