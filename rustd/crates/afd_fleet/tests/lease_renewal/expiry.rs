//! A delayed caller cannot revive either half of a lapsed runtime lease.

use super::fixture::Running;
use afd_fleet::lease::Renewed;

use crate::rows::{LAPSE, Row};
use jiff::Timestamp;

/// Which half of a runtime lease a stalled holder let lapse.
#[derive(Clone, Copy)]
enum Half {
    /// The lease row, which the runner's kill deadline reads.
    Lease,
    /// The fleet's slot, which the next claim reads.
    Slot,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_delayed_renewal_cannot_revive_an_expired_lease_or_affinity() {
    for expired_half in [Half::Lease, Half::Slot] {
        let running = Running::create().await;
        expire(&running, expired_half).await;
        let before = snapshot(&running).await;
        assert_eq!(running.renew_from_old_request().await, Renewed::Lost);
        assert_eq!(
            before,
            snapshot(&running).await,
            "refusal moves neither deadline nor accounting cursor"
        );
        running.cleanup().await;
    }
}

async fn expire(running: &Running, half: Half) {
    let row = match half {
        Half::Lease => Row::Lease(running.lease.as_str()),
        Half::Slot => Row::Slot(&running.fleet),
    };
    running.fixtures.age(row, "lease_expires_at", LAPSE).await;
}

/// The lease's deadline, the lease's meter stamp and the slot's deadline.
pub(super) async fn snapshot(running: &Running) -> (Timestamp, Timestamp, Timestamp) {
    let fixtures = &running.fixtures;
    let stamp = |row, column| async move {
        fixtures
            .instant(row, column)
            .await
            .expect("the renewal's rows carry every stamp")
    };
    (
        stamp(Row::Lease(running.lease.as_str()), "lease_expires_at").await,
        stamp(Row::Lease(running.lease.as_str()), "last_metered_at").await,
        stamp(Row::Slot(&running.fleet), "lease_expires_at").await,
    )
}
