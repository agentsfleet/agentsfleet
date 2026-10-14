//! The id a schedule is registered under, against a REAL external scheduler.
//!
//! # The bug this suite was written against
//!
//! A fire names the schedule that fell due by the id the scheduler filed it
//! under, and the fire path resolves that id to a row. The push used to let the
//! scheduler choose the id, so the fire named a schedule no row was keyed by
//! and every fire was dropped as unknown. Each later push also filed a NEW
//! schedule under a new id, leaving the previous one firing beside it. A fake
//! scheduler hides both: it answers whatever id the test expects. This suite
//! asks the compose `qstash` dev server what it actually holds.
//!
//! Self-skips outside `make test-integration-rustd`, as `integration_sync.rs`
//! does, for the reason `live_qstash.rs` gives.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/live_qstash.rs"]
mod live_qstash;

use afd_core::id::Uuid7;
use afd_cron::{
    Change, DesiredStatus, HEADER_SCHEDULE_ID, Reconciled, Schedule, ScheduleService as Reconciler,
    schedule_of, upstream_id,
};
use serde_json::Value;

use self::live_qstash::{LIVE_DESTINATION, against_live, live};
use self::support::CronLane;

/// The expression the schedule is registered with.
const FIRST_CRON: &str = "0 8 * * *";

/// The expression an edit moves it to.
const EDITED_CRON: &str = "30 9 * * *";

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn the_scheduler_files_the_schedule_under_the_id_a_fire_resolves() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let created = lane.settled("key-live-identity", FIRST_CRON).await;
    let service = against_live(&lane, url.clone(), token.clone());

    let registered = push(&lane, &service, &created.schedule_id, Change::default()).await;
    assert_eq!(
        registered.source_key,
        upstream_id(&created.schedule_id),
        "the scheduler must file the schedule under the id this daemon named"
    );
    assert_eq!(
        schedule_of(&registered.source_key),
        Some(created.schedule_id.clone()),
        "the id a fire carries back must resolve to this row"
    );
    assert_eq!(
        held_cron(&url, &token, &registered.source_key).await,
        FIRST_CRON
    );

    // An edit pushes again. The scheduler must overwrite the schedule it holds
    // under that id, not file a second one under a new id beside it.
    let edited = Change {
        cron: Some(EDITED_CRON),
        ..Change::default()
    };
    let repushed = push(&lane, &service, &created.schedule_id, edited).await;
    assert_eq!(
        repushed.source_key, registered.source_key,
        "a repeat push keeps the one registration"
    );
    assert_eq!(
        held_cron(&url, &token, &registered.source_key).await,
        EDITED_CRON,
        "the registration under the same id carries the edit"
    );

    let removed = delete(&lane, &service, &created.schedule_id).await;
    assert!(
        matches!(removed, Reconciled::Removed),
        "the registered id is the one a delete removes, got {removed:?}"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_delete_after_a_lost_answer_removes_the_registration_the_scheduler_holds() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    // The row still records the placeholder its create stored.
    let created = lane.settled("key-live-lost-answer", FIRST_CRON).await;
    let service = against_live(&lane, url.clone(), token.clone());
    let filed = upstream_id(&created.schedule_id);

    // The push the scheduler accepted and whose answer never came back: the
    // registration exists under the derived id, and the row never learned it.
    register(&url, &token, &filed).await;
    assert!(
        holds(&url, &token, &filed).await,
        "precondition: the scheduler holds it"
    );

    let removed = delete(&lane, &service, &created.schedule_id).await;
    assert!(
        matches!(removed, Reconciled::Removed),
        "a reachable scheduler confirms the delete, got {removed:?}"
    );
    assert!(
        !holds(&url, &token, &filed).await,
        "the delete must remove the registration the scheduler holds, not only the \
         placeholder the row recorded"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and the compose qstash service"]
async fn a_pause_whose_lease_ran_out_leaves_a_newer_resume_registered() {
    let Some((url, token)) = live() else {
        return;
    };
    let lane = CronLane::open().await;
    let created = lane.settled("key-live-stale-pause", FIRST_CRON).await;
    let service = against_live(&lane, url.clone(), token.clone());
    let registered = push(&lane, &service, &created.schedule_id, Change::default()).await;

    // One syncer claims a pause, then stalls past its lease before pushing it.
    let stale = CronLane::token();
    let pausing = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &created.schedule_id,
            status(DesiredStatus::Paused),
            &stale,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");
    lane.expire_lease(&created.schedule_id).await;

    // Another takes the expired row and resumes it.
    let resumed = push(
        &lane,
        &service,
        &created.schedule_id,
        status(DesiredStatus::Active),
    )
    .await;
    assert_eq!(resumed.source_key, registered.source_key);

    let overtaken = service
        .reconcile(&pausing, &stale, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");
    assert!(
        matches!(overtaken, Reconciled::Superseded),
        "the overtaken pause is refused, got {overtaken:?}"
    );
    assert!(
        holds(&url, &token, &registered.source_key).await,
        "the old pause must not remove the registration the newer resume made"
    );
}

/// A change that only moves the schedule's intent to `desired`.
fn status(desired: DesiredStatus) -> Change<'static> {
    Change {
        desired_status: Some(desired),
        ..Change::default()
    }
}

/// Claims `schedule` for deletion and reconciles it.
async fn delete(lane: &CronLane, service: &Reconciler, schedule: &Uuid7) -> Reconciled {
    let removing = Change {
        desired_status: Some(DesiredStatus::Deleting),
        ..Change::default()
    };
    let claim = CronLane::token();
    let deleting = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            schedule,
            removing,
            &claim,
            CronLane::now(),
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is claimable for deletion");
    service
        .reconcile(&deleting, &claim, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure")
}

/// Claims `schedule` with `change`, pushes it, and answers the synced row.
async fn push(
    lane: &CronLane,
    service: &Reconciler,
    schedule: &Uuid7,
    change: Change<'_>,
) -> Schedule {
    let claim = CronLane::token();
    let held = lane
        .store
        .claim_change(&lane.fleet_id(), schedule, change, &claim, CronLane::now())
        .await
        .expect("the lane's Postgres must answer")
        .expect("an unheld schedule is claimable");
    let reconciled = service
        .reconcile(&held, &claim, CronLane::now())
        .await
        .expect("a reachable scheduler is not a datastore failure");
    let Reconciled::Synced(row) = reconciled else {
        panic!("the scheduler must accept the push, got {reconciled:?}");
    };
    row
}

/// The expression the scheduler holds for the schedule filed under `id`.
async fn held_cron(base: &str, token: &str, id: &str) -> String {
    let answer = read(base, token, id).await;
    assert!(
        answer.status().is_success(),
        "the scheduler holds no schedule under {id}: {}",
        answer.status()
    );
    let held: Value =
        serde_json::from_str(&answer.text().await.expect("the scheduler answers a body"))
            .expect("the scheduler answers JSON");
    held.get("cron")
        .and_then(Value::as_str)
        .expect("a held schedule carries its expression")
        .to_owned()
}

/// Whether the scheduler holds a schedule filed under `id`.
async fn holds(base: &str, token: &str, id: &str) -> bool {
    read(base, token, id).await.status().is_success()
}

/// The scheduler's answer to reading the schedule filed under `id`.
#[expect(
    clippy::disallowed_methods,
    reason = "the lane reaches its live scheduler with reqwest's defaults, as `live_qstash.rs` does"
)]
async fn read(base: &str, token: &str, id: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{base}/schedules/{id}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("the lane's scheduler answers")
}

/// Registers a schedule under `id` directly, as a push whose answer was lost
/// left it.
#[expect(
    clippy::disallowed_methods,
    reason = "the lane reaches its live scheduler with reqwest's defaults, as `live_qstash.rs` does"
)]
async fn register(base: &str, token: &str, id: &str) {
    let answer = reqwest::Client::new()
        .post(format!("{base}/schedules/{LIVE_DESTINATION}"))
        .bearer_auth(token)
        .header(HEADER_SCHEDULE_ID, id)
        .header("Upstash-Cron", FIRST_CRON)
        .body("check the deploys")
        .send()
        .await
        .expect("the lane's scheduler answers");
    assert!(
        answer.status().is_success(),
        "the scheduler refused the direct registration: {}",
        answer.status()
    );
}
