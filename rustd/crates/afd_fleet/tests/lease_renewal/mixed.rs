//! Mixed kinds meet one lapsed slot on a one-worker host.
//!
//! Replica A issued a runtime lease and stalls past its expiry. Replica B, on
//! its own pool, admits a queued authoring turn into the freed slot while A's
//! delayed renewal is still in flight. In every ordering the authoring turn is
//! the only live lease, the late renewal is refused without reviving either
//! half of A's lease, the host stays at its worker limit, nothing waits
//! forever, and the next placement offers the runtime kind first.

use super::{
    expiry::snapshot, fixture::Running, holders::assert_host_full, replica::expire,
    replicas::contended, seat::Seat,
};
use afd_authoring::{Claimed, Drafts, types::Draft};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use afd_fleet::lease::Renewed;
use afd_wire::lease::{AcceptedKinds, LeaseKind};
use std::time::Duration;

/// Every ordering finishes inside this, or the test fails rather than hangs.
const DEADLOCK_BOUND: Duration = Duration::from_secs(30);
const LIVE_AUTHORING: &str = const_format::concatcp!(
    "SELECT lease_id::text FROM fleet.authoring_attempts WHERE runner_id = $1::uuid \
     AND status = $2 AND lease_expires_at > ",
    afd_db::statement::NOW_MS,
);

/// Where A's late renewal lands relative to B's authoring admission.
#[derive(Debug, Clone, Copy)]
enum Late {
    /// Answered before B admits at all.
    Answered,
    /// Both wait on the host lock, the renewal queued ahead of the admission.
    QueuedAhead,
    /// Both wait on the host lock, the admission queued ahead of the renewal.
    QueuedBehind,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_expired_runtime_renewal_and_authoring_admission_share_one_slot() {
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
    let seat = Seat::create(&running).await;
    let replica = AuthoringReplica::start().await;
    let draft = seat.queue(&replica.drafts).await;
    expire(&running).await;
    let expired = snapshot(&running).await;
    let label = format!("{order:?}");
    let (admitted, renewed) = Box::pin(race(order, &running, &replica, &draft)).await;
    assert_eq!(
        renewed,
        Renewed::Lost,
        "{label}: the lapsed runtime holder is refused"
    );
    let admitted = admitted.expect("in every ordering, authoring takes the freed slot");
    assert_eq!(
        snapshot(&running).await,
        expired,
        "{label}: the refusal revived neither deadline nor meter"
    );
    assert_eq!(
        live_authoring(&running).await,
        std::slice::from_ref(&admitted.lease_id),
        "{label}: the admitted turn is the host's only live authoring lease"
    );
    assert_host_full(&running, &label).await;
    let next = afd_runner::Runners::new(running.fixtures.database.clone(), Entropy::new())
        .claim_offer(&running.runner, AcceptedKinds::ALL)
        .await
        .expect("the next poll reads its preference")
        .expect("the host exists");
    assert_eq!(
        next.order().next(),
        Some(LeaseKind::Fleet),
        "{label}: the runtime kind is offered first after an authoring placement"
    );
    replica.stop().await;
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}

/// A's late renewal and B's admission, in the order `order` names.
async fn race(
    order: Late,
    running: &Running,
    replica: &AuthoringReplica,
    draft: &Draft,
) -> (Option<Claimed>, Renewed) {
    let admit = || Seat::admit(&replica.drafts, &running.runner, draft);
    match order {
        Late::Answered => {
            let renewed = running.renew_from_old_request().await;
            (admit().await, renewed)
        }
        Late::QueuedAhead => {
            let (renewed, admitted) = Box::pin(contended(
                running,
                running.renew_from_old_request(),
                admit(),
            ))
            .await;
            (admitted, renewed)
        }
        Late::QueuedBehind => {
            Box::pin(contended(
                running,
                admit(),
                running.renew_from_old_request(),
            ))
            .await
        }
    }
}

async fn live_authoring(running: &Running) -> Vec<String> {
    sqlx::query_scalar(LIVE_AUTHORING)
        .bind(running.runner.as_str())
        .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
        .fetch_all(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("live authoring leases")
}

/// The second daemon replica: its own pool, meeting A only in rows.
struct AuthoringReplica {
    lane: TestDatabase,
    database: Db,
    drafts: Drafts,
}

impl AuthoringReplica {
    async fn start() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        Self {
            drafts: Seat::drafts(&database),
            lane,
            database,
        }
    }

    async fn stop(self) {
        self.database.close().await;
        self.lane.cleanup().await;
    }
}
