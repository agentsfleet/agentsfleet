//! Two daemon replicas meet one expired lease: one re-admits, one renews late.
//!
//! Replica A issued the lease and stalls past its expiry. Replica B's poll
//! re-admits the same event on the same one-worker host while A's delayed
//! renewal is still in flight. Every ordering must end with B the only live
//! holder, A refused, the host at its worker limit and nothing waiting forever.

use super::{
    expiry::snapshot,
    fixture::Running,
    holders::{Outcome, assert_one_holder},
    replica::{Replica, expire, original},
};
use afd_core::id::Uuid7;
use afd_fleet::lease::Renewed;
use sqlx::{Connection as _, Postgres, Transaction};
use std::time::Duration;

/// Every ordering finishes inside this, or the test fails rather than hangs.
const DEADLOCK_BOUND: Duration = Duration::from_secs(30);

const HOLD_HOST: &str =
    "SELECT pg_backend_pid() FROM runner.runners WHERE id = $1::uuid FOR UPDATE";
/// Transitive: a second row-lock waiter queues behind the first, not the holder.
const WAITERS: &str = "\
WITH RECURSIVE blocked(pid) AS (
 SELECT $1::integer UNION
 SELECT a.pid FROM pg_stat_activity a JOIN blocked b ON b.pid = ANY(pg_blocking_pids(a.pid))
)
SELECT count(*) FROM blocked WHERE pid <> $1";

/// Where A's late renewal lands relative to B's re-admission.
#[derive(Debug, Clone, Copy)]
enum Late {
    /// Answered before B polls at all.
    Answered,
    /// Both wait on the host lock, the renewal queued ahead of the admission.
    QueuedAhead,
    /// Both wait on the host lock, the admission queued ahead of the renewal.
    QueuedBehind,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_replicas_expiry_readmission_and_late_renewal_leave_one_holder() {
    for order in [Late::Answered, Late::QueuedAhead, Late::QueuedBehind] {
        let finished = Box::pin(tokio::time::timeout(DEADLOCK_BOUND, scenario(order))).await;
        assert!(
            finished.is_ok(),
            "{order:?} deadlocked or starved past the bound"
        );
    }
}

async fn scenario(order: Late) {
    let running = Running::create().await;
    let replica = Replica::start().await;
    let (original_event, original_status) = original(&running).await;
    assert_eq!(original_status, afd_runner::sql::LEASE_STATUS_ACTIVE);
    expire(&running).await;
    let expired = snapshot(&running).await;
    let ((event, readmitted), renewed) = Box::pin(race(order, &running, &replica)).await;
    let label = format!("{order:?}");
    assert_eq!(
        renewed,
        Renewed::Lost,
        "{label}: the late renewal is refused"
    );
    assert_eq!(
        event, original_event,
        "{label}: the same work is re-admitted"
    );
    assert_ne!(
        readmitted, running.lease,
        "{label}: re-admission is a new lease"
    );
    let outcome = Outcome {
        order: &label,
        original_event: &original_event,
        readmitted: &readmitted,
        expired,
    };
    assert_one_holder(&running, &replica, &outcome).await;
    replica.stop().await;
    running.cleanup().await;
}

/// A's late renewal and B's re-admission, in the order `order` names.
async fn race(order: Late, running: &Running, replica: &Replica) -> ((String, Uuid7), Renewed) {
    match order {
        Late::Answered => {
            let renewed = running.renew_from_old_request().await;
            assert_eq!(renewed, Renewed::Lost, "{order:?}: expiry alone refuses it");
            (replica.readmit(running).await, renewed)
        }
        Late::QueuedAhead => {
            let (renewed, readmitted) = Box::pin(contended(
                running,
                running.renew_from_old_request(),
                replica.readmit(running),
            ))
            .await;
            (readmitted, renewed)
        }
        Late::QueuedBehind => {
            Box::pin(contended(
                running,
                replica.readmit(running),
                running.renew_from_old_request(),
            ))
            .await
        }
    }
}

/// Hold the host row, queue `first` then `second` behind it, then release both.
///
/// Waiting on `pg_blocking_pids` fixes the queue order without a sleep: the
/// second request starts only once the first is observed waiting on the lock.
pub(super) async fn contended<A, B>(
    running: &Running,
    first: impl Future<Output = A>,
    second: impl Future<Output = B>,
) -> (A, B) {
    let mut connection = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("host lock holder");
    let mut held = connection.begin().await.expect("open host blocker");
    let blocker: i32 = sqlx::query_scalar(HOLD_HOST)
        .bind(running.runner.as_str())
        .fetch_one(&mut *held)
        .await
        .expect("hold the shared host row");
    let later = async {
        waiting(running, blocker, 1).await;
        second.await
    };
    let release = async {
        waiting(running, blocker, 2).await;
        release(held).await;
    };
    let (first, second, ()) = tokio::join!(first, later, release);
    drop(connection);
    (first, second)
}

async fn release(held: Transaction<'_, Postgres>) {
    held.commit()
        .await
        .expect("release the host to both replicas");
}

pub(super) async fn waiting(running: &Running, blocker: i32, count: i64) {
    let mut observer = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("observe lock waiters");
    while sqlx::query_scalar::<_, i64>(WAITERS)
        .bind(blocker)
        .fetch_one(&mut *observer)
        .await
        .expect("inspect lock waiters")
        < count
    {
        tokio::task::yield_now().await;
    }
}
