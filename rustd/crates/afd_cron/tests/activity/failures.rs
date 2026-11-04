//! Refused and superseded mutations cannot announce success.

use std::pin::Pin;

use afd_cron::{Change, DesiredStatus, Refused, SyncStatus};
use afd_db::Precondition;
use serde_json::json;
use sqlx::PgConnection;

use super::fixture::*;
use crate::support::{CronLane, SEED_AT};

struct NotHeld;
impl Precondition for NotHeld {
    fn holds<'a>(
        &'a self,
        _connection: &'a mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'a>> {
        Box::pin(std::future::ready(Ok(false)))
    }
}

// Value: protects=failure reason and recovery are durable while duplicate finalization is inert;
// fails_when=failure is omitted, replay adds history or recovery retains last_error;
// seam=native store confirms supplied upstream facts; no_claim=vendor transport.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failed_schedule_sync_history_recovers_without_duplicate_success() {
    let lane = CronLane::open().await;
    let token = CronLane::token();
    let created = create(&lane, &token).await;
    let id = &created.schedule_id;
    let reason = "The scheduler could not be reached.";
    let failed = lane
        .store
        .finalize_failed(&created, &token, reason)
        .await
        .expect("record failure")
        .expect("current claim");
    assert_eq!(
        (failed.sync_status, failed.last_error.as_deref()),
        (SyncStatus::Failed, Some(reason))
    );
    assert_eq!(
        lane.store
            .finalize_failed(&created, &token, reason)
            .await
            .expect("replay"),
        None
    );
    let retry = CronLane::token();
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), id, &retry)
        .await
        .expect("retry")
        .expect("claim");
    let recovered = confirm(&lane, &held, &retry).await;
    assert_eq!(
        (recovered.sync_status, recovered.last_error.as_deref()),
        (SyncStatus::Synced, None)
    );
    same_facts(
        receipts(&lane).await,
        vec![
            changed(id, "created", "active", MESSAGE),
            json!({"kind":"schedule_sync_failed","schedule_id":created.schedule_id.as_str(),
            "description":DESCRIPTION,"message":MESSAGE,"status":"active","sync":"failed","last_error":reason}),
            fact(id, "schedule_registered", "active", "synced", MESSAGE),
        ],
    );
    no_work(&lane).await;
}

// Value: protects=a superseded claim cannot change state or append any success/failure/deletion;
// fails_when=receipt creation runs independently of the fenced mutation;
// seam=stored expiry denotes an abandoned holder; no_claim=wall-clock waiting.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn superseded_schedule_sync_emits_no_successful_activity() {
    let lane = CronLane::open().await;
    let old = CronLane::token();
    let created = create(&lane, &old).await;
    lane.expire_lease(&created.schedule_id).await;
    let current = CronLane::token();
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &current)
        .await
        .expect("reclaim")
        .expect("new holder");
    assert_eq!(
        lane.store
            .finalize_synced(&created, &old, None)
            .await
            .expect("late success"),
        None
    );
    assert_eq!(
        lane.store
            .finalize_failed(&created, &old, "Old failure")
            .await
            .expect("late failure"),
        None
    );
    assert!(
        !lane
            .store
            .delete_claimed(&created, &old)
            .await
            .expect("late deletion")
    );
    assert_eq!(
        lane.store
            .one(&lane.fleet_id(), &created.schedule_id)
            .await
            .expect("unchanged holder"),
        Some(held)
    );
    same_facts(
        receipts(&lane).await,
        vec![changed(&created.schedule_id, "created", "active", MESSAGE)],
    );
    no_work(&lane).await;
}

// Value: protects=foreign ownership and lost lease authority refuse state and history together;
// fails_when=receipt insertion precedes the proven owner/guard;
// seam=fixed native Precondition verdict; no_claim=the runner's actual lease guard.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn refused_schedule_changes_leave_history_and_state_unchanged() {
    let lane = CronLane::open().await;
    let foreign = CronLane::open().await;
    let fleet = lane.fleet_id();
    let token = CronLane::token();
    let denied = lane
        .store
        .create_guarded(&lane.workspace_id(), new(&fleet), &token, SEED_AT, &NotHeld)
        .await
        .expect("native guard");
    assert_eq!(denied, Err(Refused::Unheld));
    let wrong = lane
        .store
        .create(&foreign.workspace_id(), new(&fleet), &token, SEED_AT)
        .await
        .expect("owner check");
    assert_eq!(wrong, Err(Refused::NoSuchFleet));
    assert_eq!((lane.count().await, receipts(&lane).await), (0, vec![]));
    let row = registered(&lane).await;
    let before = receipts(&lane).await;
    let denied = lane
        .store
        .claim_change_guarded(
            &fleet,
            &row.schedule_id,
            Change {
                desired_status: Some(DesiredStatus::Paused),
                ..Change::default()
            },
            &CronLane::token(),
            &NotHeld,
        )
        .await
        .expect("lost guard");
    assert_eq!(denied, Err(Refused::Unheld));
    assert_eq!(
        lane.store
            .one(&fleet, &row.schedule_id)
            .await
            .expect("reload"),
        Some(row)
    );
    same_facts(receipts(&lane).await, before);
    assert_eq!(receipts(&foreign).await, Vec::<serde_json::Value>::new());
    no_work(&lane).await;
}
