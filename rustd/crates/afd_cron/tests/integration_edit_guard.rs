//! A person's schedule edit, against a row that is not free to take it, and
//! against a one-off whose instant the edit moves.
//!
//! The two refusals are proved with the tag the caller would really hold, so a
//! pass means the row's state refused the edit and not a stale version.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_api_wire::schedule::Patch;
use afd_cron::error::detail::{DELETING, SYNC_HELD};
use afd_cron::{Change, DesiredStatus, EditRequest, NewSchedule, Schedule, Source};
use jiff::tz::TimeZone;

use self::support::{CronLane, SEED_AT};

/// 09:05 UTC on 15 March: the one-off's minute, whichever year is next.
const ONCE_CRON: &str = "5 9 15 3 *";

/// The same day at 09:10, where the edit moves it.
const MOVED_CRON: &str = "10 9 15 3 *";

fn patch(cron: &str) -> Patch {
    serde_json::from_value(serde_json::json!({ "cron": cron })).expect("an edit")
}

/// Edits `schedule`'s expression to `cron`, with the tag it carries now.
async fn edit(lane: &CronLane, schedule: &Schedule, cron: &str) -> afd_cron::Result<Schedule> {
    let (workspace, fleet) = (lane.workspace_id(), lane.fleet_id());
    let etag = schedule.etag();
    let patch = patch(cron);
    lane.store
        .edit(
            EditRequest {
                workspace: &workspace,
                fleet: &fleet,
                schedule: &schedule.schedule_id,
                expected: Some(&etag),
                patch: &patch,
            },
            &CronLane::token(),
        )
        .await
}

/// `schedule` as the table holds it now.
async fn reread(lane: &CronLane, schedule: &Schedule) -> Schedule {
    lane.store
        .one(&lane.fleet_id(), &schedule.schedule_id)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is still there")
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn an_edit_while_a_sync_holds_the_row_is_refused_and_writes_nothing() {
    let lane = CronLane::open().await;
    // Born held: the creating request keeps the fence until it has pushed.
    let held = lane.create("key-edit-held", "0 3 * * *").await;

    let refused = edit(&lane, &held, "0 4 * * *")
        .await
        .expect_err("a row a live syncer holds must not take an edit");

    assert_eq!(refused.detail(), SYNC_HELD);
    assert_eq!(reread(&lane, &held).await.cron, "0 3 * * *");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn an_edit_to_a_schedule_being_removed_is_refused() {
    let lane = CronLane::open().await;
    let settled = lane.settled("key-edit-deleting", "0 3 * * *").await;
    let remover = CronLane::token();
    let deleting = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &settled.schedule_id,
            Change {
                desired_status: Some(DesiredStatus::Deleting),
                ..Change::default()
            },
            &remover,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the schedule is claimable for deletion");
    // The remover stalled: its lease lapsed, so only the intent still says no.
    lane.expire_lease(&deleting.schedule_id).await;
    let deleting = reread(&lane, &deleting).await;

    let refused = edit(&lane, &deleting, "0 4 * * *")
        .await
        .expect_err("a schedule on its way out must not be edited back to life");

    assert_eq!(refused.detail(), DELETING);
    let stored = reread(&lane, &deleting).await;
    assert_eq!(stored.desired_status, DesiredStatus::Deleting);
    assert_eq!(stored.cron, "0 3 * * *");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn an_edit_to_a_one_off_recounts_its_instant_from_the_edit() {
    let lane = CronLane::open().await;
    let fleet = lane.fleet_id();
    let creator = CronLane::token();
    let created = lane
        .store
        .create(
            &lane.workspace_id(),
            NewSchedule {
                fleet: &fleet,
                source: Source::Fleet,
                provider_schedule_id: None,
                cron: ONCE_CRON,
                timezone: "UTC",
                message: "check the deploy",
                once: true,
            },
            &creator,
            SEED_AT,
        )
        .await
        .expect("the lane's Postgres must answer")
        .expect("the one-off is admitted");
    let settled: Schedule = lane
        .store
        .finalize_synced(&created, &creator, None)
        .await
        .expect("the lane's Postgres must answer")
        .expect("the creator's own finalize lands");

    let edited = edit(&lane, &settled, MOVED_CRON)
        .await
        .expect("an unheld one-off takes an edit");

    let fire_at = edited.fire_at.expect("an edited one-off keeps an instant");
    assert!(
        fire_at > edited.updated_at,
        "the instant is counted from the edit's own stamp"
    );
    let civil = fire_at.to_zoned(TimeZone::UTC);
    assert_eq!(
        (civil.month(), civil.day(), civil.hour(), civil.minute()),
        (3, 15, 9, 10),
        "the instant is the MOVED minute, not the one it was created for"
    );
}
