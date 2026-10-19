//! Dimension 4.8 for schedules: a sync lease that lapses while a guard waits on
//! the schedule's row lock no longer holds the schedule once the guard has it.
//!
//! The holder only locks the row and writes nothing. A holder that wrote it
//! would make Postgres re-check the predicate after the wait and hide the
//! defect this guards against: a deadline compared with a clock read before
//! the wait.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use std::time::Duration;

use afd_api_wire::schedule::Patch;
use afd_cron::{EditRequest, Recipe, RecipeSet};
use sqlx::{Connection as _, Postgres, Transaction};

use self::support::{CronLane, SEED_AT};

/// How long the sync lease has left when the row lock is taken: long enough
/// for the guard to queue behind the lock before the lease lapses.
const LAPSES_IN: Duration = Duration::from_millis(500);

/// Hands the schedule to another syncer whose lease lapses `$3` past
/// database time.
const LEASE_LAPSES: &str = "UPDATE fleet.schedules \
SET sync_token = $2::uuid, \
    sync_lease_expires_at = date_trunc('milliseconds', clock_timestamp()) + $3::interval \
WHERE id = $1::uuid";

/// Takes the schedule row's lock and writes nothing.
const LOCK_SCHEDULE: &str =
    "SELECT pg_backend_pid() FROM fleet.schedules WHERE id = $1::uuid FOR UPDATE";

/// Whether the schedule's sync lease is still live on database time.
const LEASE_LIVE: &str =
    "SELECT sync_lease_expires_at > clock_timestamp() FROM fleet.schedules WHERE id = $1::uuid";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// The one recipe-carrying schedule on the lane's fleet.
const RECIPE_SCHEDULE: &str =
    "SELECT id::text FROM fleet.schedules WHERE fleet_id = $1::uuid AND recipe IS NOT NULL";

/// Commits another syncer's lease on `schedule`, lapsing in [`LAPSES_IN`],
/// then takes the row's lock on `held`; answers the holder's backend.
async fn lapse_and_lock(
    lane: &CronLane,
    held: &mut Transaction<'_, Postgres>,
    schedule: &str,
) -> i32 {
    sqlx::query(LEASE_LAPSES)
        .bind(schedule)
        .bind(CronLane::token().as_str())
        .bind(LAPSES_IN)
        .execute(&mut *lane.connection().await)
        .await
        .expect("another syncer holds the schedule");
    sqlx::query_scalar(LOCK_SCHEDULE)
        .bind(schedule)
        .fetch_one(&mut **held)
        .await
        .expect("take the schedule's row lock")
}

/// Holds the row lock while the guard queues behind it, past the lease's
/// deadline, then lets go without writing.
///
/// Asserts the guard queued while the lease was still live, so a pass means it
/// lapsed during the wait and not before the guard began.
async fn hold_past_deadline(
    lane: &CronLane,
    held: Transaction<'_, Postgres>,
    pid: i32,
    schedule: &str,
) {
    let mut probe = lane.connection().await;
    loop {
        let waiting: bool = sqlx::query_scalar(BLOCKING)
            .bind(pid)
            .fetch_one(&mut *probe)
            .await
            .expect("inspect the lock queue");
        if waiting {
            break;
        }
        tokio::task::yield_now().await;
    }
    let live: bool = sqlx::query_scalar(LEASE_LIVE)
        .bind(schedule)
        .fetch_one(&mut *probe)
        .await
        .expect("read the lease");
    assert!(live, "the guard must queue before the lease lapses");
    tokio::time::sleep(LAPSES_IN).await;
    let live: bool = sqlx::query_scalar(LEASE_LIVE)
        .bind(schedule)
        .fetch_one(&mut *probe)
        .await
        .expect("read the lease");
    assert!(!live, "the lease lapses while the row lock is held");
    held.rollback()
        .await
        .expect("the holder lets go without writing");
}

/// Replaces the fleet's recipes with one, on its own transaction.
async fn replace_recipes(lane: &CronLane, recipes: &[Recipe]) -> afd_cron::Result<()> {
    let (workspace, fleet) = (lane.workspace_id(), lane.fleet_id());
    let mut connection = lane.connection().await;
    let mut tx = connection.begin().await.expect("recipe transaction");
    lane.store
        .replace_recipes_on(
            &mut tx,
            RecipeSet {
                workspace: &workspace,
                fleet: &fleet,
                setup_revision: 1,
                recipes,
                now: SEED_AT,
            },
        )
        .await?;
    tx.commit().await.expect("commit the recipes");
    Ok(())
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn test_expiry_checks_read_clock_after_lock() {
    let lane = CronLane::open().await;
    let (workspace, fleet) = (lane.workspace_id(), lane.fleet_id());

    // A schedule edit: the row lock, then the sync-lease read.
    let created = lane.settled("key-lapse-edit", "0 3 * * *").await;
    let schedule = created.schedule_id.as_str().to_owned();
    let patch: Patch =
        serde_json::from_value(serde_json::json!({"cron":"0 5 * * *"})).expect("an edit");
    let etag = created.etag();
    let editor = CronLane::token();
    let mut holder = lane.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&lane, &mut held, &schedule).await;
    let request = EditRequest {
        workspace: &workspace,
        fleet: &fleet,
        schedule: &created.schedule_id,
        expected: Some(&etag),
        patch: &patch,
    };
    let (edited, ()) = tokio::join!(
        lane.store.edit(request, &editor),
        hold_past_deadline(&lane, held, pid, &schedule)
    );
    let edited = edited.expect("a lease that lapsed in the wait no longer holds the schedule");
    assert_eq!(edited.cron, "0 5 * * *");
    assert_eq!(edited.sync_token.as_deref(), Some(editor.as_str()));

    // A recipe replacement: one locking read judges every inherited row.
    let recipes =
        [Recipe::parse("0 4 * * *", "UTC", "run the inherited repair").expect("a recipe")];
    replace_recipes(&lane, &recipes)
        .await
        .expect("the first recipe lands");
    let inherited: String = sqlx::query_scalar(RECIPE_SCHEDULE)
        .bind(fleet.as_str())
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("the recipe's schedule");
    let mut held = holder.begin().await.expect("holder transaction");
    let pid = lapse_and_lock(&lane, &mut held, &inherited).await;
    let (replaced, ()) = tokio::join!(
        replace_recipes(&lane, &recipes),
        hold_past_deadline(&lane, held, pid, &inherited)
    );
    replaced.expect("a lease that lapsed in the wait no longer holds the recipe");
}
