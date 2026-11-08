//! Complete lifecycle snapshots, including confirmed absence after removal.

use super::fixture::*;
use crate::support::{CronLane, SEED_AT};
use afd_cron::{Change, DesiredStatus, SyncStatus};

// Value: protects=stored intent and registration remain distinct, durable facts;
// fails_when=state commits without history or claims registration early;
// seam=none; no_claim=external scheduler or browser rendering.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn schedule_mutations_commit_history_with_their_rows() {
    let lane = CronLane::open().await;
    let token = CronLane::token();
    let row = create(&lane, &token).await;
    let id = &row.schedule_id;
    same_facts(
        receipts(&lane).await,
        vec![changed(id, "created", "active", MESSAGE)],
    );
    let registered = confirm(&lane, &row, &token).await;
    assert_eq!(
        (registered.sync_status, registered.last_error.as_deref()),
        (SyncStatus::Synced, None)
    );
    let token = CronLane::token();
    let pause = Change {
        desired_status: Some(DesiredStatus::Paused),
        ..Change::default()
    };
    let paused = change(&lane, &row, pause, &token).await;
    let confirmed = confirm(&lane, &paused, &token).await;
    assert_eq!(
        (confirmed.desired_status, confirmed.sync_status),
        (DesiredStatus::Paused, SyncStatus::Synced)
    );
    assert_eq!(stored(&lane, id).await, Some(confirmed));
    same_facts(
        receipts(&lane).await,
        vec![
            changed(id, "created", "active", MESSAGE),
            fact(id, "schedule_registered", "active", "synced", MESSAGE),
            changed(id, "paused", "paused", MESSAGE),
            fact(id, "schedule_unregistered", "paused", "synced", MESSAGE),
        ],
    );
    no_work(&lane).await;
}

// Value: protects=confirmed deletion retains its snapshot and replays are inert;
// fails_when=removal loses text or a stale finalize appends history;
// seam=none; no_claim=upstream removal without native caller confirmation.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn schedule_removal_commits_confirmed_history_once() {
    let lane = CronLane::open().await;
    let row = registered(&lane).await;
    let id = &row.schedule_id;
    let token = CronLane::token();
    let remove = Change {
        desired_status: Some(DesiredStatus::Deleting),
        ..Change::default()
    };
    let deleting = change(&lane, &row, remove, &token).await;
    assert!(
        lane.store
            .delete_claimed(&deleting, &token)
            .await
            .expect("confirmed deletion")
    );
    assert!(
        !lane
            .store
            .delete_claimed(&deleting, &token)
            .await
            .expect("replay")
    );
    assert_eq!(stored(&lane, id).await, None);
    same_facts(
        receipts(&lane).await,
        vec![
            changed(id, "created", "active", MESSAGE),
            fact(id, "schedule_registered", "active", "synced", MESSAGE),
            changed(id, "removal_requested", "deleting", MESSAGE),
            fact(id, "schedule_removed", "removed", "synced", MESSAGE),
        ],
    );
    no_work(&lane).await;
}

// Value: protects=a missed one-off records expiry without claiming registration/fire;
// fails_when=active once removal is labelled registered, fired or ordinary deletion;
// seam=backdated fire_at is a reachable missed instant; no_claim=vendor delivery.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn missed_one_off_expiry_keeps_its_message_without_a_run() {
    let lane = CronLane::open().await;
    let token = CronLane::token();
    let fleet = lane.fleet_id();
    let mut request = new(&fleet);
    request.once = true;
    let row = lane
        .store
        .create(&lane.workspace_id(), request, &token, SEED_AT)
        .await
        .expect("store once")
        .expect("accepted");
    let id = &row.schedule_id;
    lane.backdate_fire_at(id).await;
    let missed = stored(&lane, id).await.expect("missed row");
    assert!(
        lane.store
            .delete_claimed(&missed, &token)
            .await
            .expect("confirmed removal")
    );
    assert_eq!(stored(&lane, id).await, None);
    same_facts(
        receipts(&lane).await,
        vec![
            changed(id, "created", "active", MESSAGE),
            fact(id, "schedule_expired", "expired", "synced", MESSAGE),
        ],
    );
    no_work(&lane).await;
}
