//! A runaway trip archives the fleet AND ends the event that tripped it.
//!
//! Archive is terminal, so the triggering event must not be left waiting on a
//! fleet that will never run it again: it settles as ended, under its own
//! label, through the same refusal a human's no takes.

use super::seed::seed_provider_resolution;
use super::*;

use af_core::event::label;

/// A fleet whose first repeated action already crosses its runaway threshold.
const RUNAWAY_AT_ONE: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"},"gates":{"anomaly_rules":[{"pattern":"same_action","threshold_count":1,"threshold_window_s":60}]}}}"#;

/// The stored status of an archived fleet.
const ARCHIVED: &str = "archived";

// Value: protects=a runaway's event cannot sit pending forever; fails_when=the
// archived verdict retries the event instead of ending it; why_new=archive
// became terminal; seam=Postgres; no_claim=the gate's archive write itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_runaway_trip_archives_the_fleet_and_ends_its_event() {
    let fixtures = Fixtures::create().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, RUNAWAY_AT_ONE).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(answer.is_none(), "a runaway issued a lease: {answer:?}");
    let status: String = sqlx::query_scalar("SELECT status FROM fleet.fleets WHERE id = $1")
        .bind(&seeded.fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("fleet read"))
        .await
        .expect("the archived fleet stays readable");
    assert_eq!(status, ARCHIVED, "the runaway archived its fleet");
    let (status, failure) = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id)
        .await
        .expect("the tripping event has a settled row, not a pending one");
    assert_eq!(status, STATUS_GATE_BLOCKED, "the event is ended");
    assert_eq!(failure, label::FLEET_ARCHIVED, "under its own label");
    fixtures.cleanup().await;
}
