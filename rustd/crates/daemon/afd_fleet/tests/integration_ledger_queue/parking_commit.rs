//! An answer or expiry waits for a park that has written but not committed.

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_db::config::DbRole;
use sqlx::Row as _;

use super::{WITHIN, enqueue, next, parking::seed_gate};
use crate::{seed::seeded_parts, support::Fixtures};

const PAUSE_PARK: &str = "CREATE FUNCTION fleet.pause_park() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      PERFORM pg_advisory_xact_lock(hashtextextended(NEW.fleet_id::text, 0)); \
      RETURN NEW; END $$; \
    CREATE TRIGGER pause_park AFTER UPDATE OF parked_at ON fleet.admissions \
    FOR EACH ROW WHEN (NEW.parked_at IS NOT NULL) EXECUTE FUNCTION fleet.pause_park()";
const LOCK: &str = "SELECT pg_advisory_lock(hashtextextended($1,0))";
const UNLOCK: &str = "SELECT pg_advisory_unlock(hashtextextended($1,0))";
const WAITER: &str = "SELECT pid FROM pg_stat_activity \
    WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)) LIMIT 1";

// Value: protects=answered heads remain runnable; fails_when=resolver skips an
// uncommitted park; why_new=answer-first tests miss uncommitted park writes;
// seam=none; no_claim=all gate interleavings. A database trigger holds the write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn answers_and_expiry_clear_a_park_that_was_uncommitted_when_resolution_started() {
    for expire in [false, true] {
        resolve_during_park(expire).await;
    }
}

async fn resolve_during_park(expire: bool) {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let acquired = next(&leases, &runner).await.expect("claim");
    let action = seed_gate(&fixtures, &fleet, &workspace).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut blocker = owner.acquire().await.expect("owner");
    sqlx::raw_sql(PAUSE_PARK)
        .execute(&mut *blocker)
        .await
        .expect("pause trigger");
    sqlx::query(LOCK)
        .bind(&fleet)
        .execute(&mut *blocker)
        .await
        .expect("hold pause");
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .expect("pause owner");
    let parking_event = event.clone();
    let parking = tokio::spawn(async move {
        leases
            .park(&acquired.fleet_id, &parking_event, acquired.fence)
            .await
    });
    let park_pid = waiter(&fixtures, blocker_pid, None)
        .await
        .expect("park is blocked");
    let resolving = start_resolution(&fixtures, action, expire).await;
    let _wait = waiter(&fixtures, park_pid, Some(&resolving)).await;
    sqlx::query(UNLOCK)
        .bind(&fleet)
        .execute(&mut *blocker)
        .await
        .expect("release park");
    parking.await.expect("park task").expect("park commits");
    resolving.await.expect("resolution task");
    let parked: bool =
        sqlx::query_scalar("SELECT parked_at IS NOT NULL FROM fleet.admissions WHERE id=$1::uuid")
            .bind(&event)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("committed park state");
    assert!(
        !parked,
        "expire={expire}: a resolved gate cannot strand its head"
    );
    assert_eq!(
        next(&fixtures.leases_with_dead_queue(), &runner)
            .await
            .expect("head runnable")
            .event_id,
        event
    );
    drop(blocker);
    owner.close().await;
    fixtures.cleanup().await;
}

async fn start_resolution(
    fixtures: &Fixtures,
    action: String,
    expire: bool,
) -> tokio::task::JoinHandle<()> {
    if expire {
        sqlx::query("UPDATE fleet.approval_gates SET expires_at=now()-interval '1 second' WHERE action_id=$1")
            .bind(&action).execute(&mut *fixtures.database.acquire().await.expect("age gate"))
            .await.expect("gate expires");
    }
    let queue = crate::queue::unreachable();
    let inbox = Inbox::new(
        fixtures.database.clone(),
        queue.clone(),
        Admissions::for_tests(fixtures.database.clone(), queue),
    );
    tokio::spawn(async move {
        if expire {
            assert_eq!(inbox.expire().await.expect("expiry commits"), 1);
        } else {
            assert!(matches!(
                inbox
                    .resolve(&action, Decision::Approved, "fixture:human", "", None)
                    .await
                    .expect("answer commits"),
                Resolution::Resolved(_)
            ));
        }
    })
}

/// Proceed only once the resolver is blocked on the park or has already returned.
async fn waiter(
    fixtures: &Fixtures,
    blocker: i32,
    task: Option<&tokio::task::JoinHandle<()>>,
) -> Option<i32> {
    let mut observer = fixtures.database.acquire().await.expect("lock observer");
    tokio::time::timeout(WITHIN, async {
        loop {
            if let Some(row) = sqlx::query(WAITER)
                .bind(blocker)
                .fetch_optional(&mut *observer)
                .await
                .expect("observe lock")
            {
                return Some(row.get("pid"));
            }
            if task.is_some_and(tokio::task::JoinHandle::is_finished) {
                return None;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("operation reaches lock boundary")
}
