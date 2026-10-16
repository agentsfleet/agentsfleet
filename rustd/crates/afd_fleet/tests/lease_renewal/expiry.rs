//! A delayed caller cannot revive either half of a lapsed runtime lease.

use super::fixture::Running;
use afd_billing::{Cumulative, Meter, SliceRates};
use afd_fleet::lease::Renewed;

const EXPIRE_LEASE: &str = "\
UPDATE fleet.runner_leases SET lease_expires_at = $2 WHERE id = $1::uuid";
const EXPIRE_AFFINITY: &str = "\
UPDATE fleet.runner_affinity SET leased_until = $2 WHERE fleet_id = $1::uuid";
const READ: &str = "\
SELECT l.lease_expires_at, l.last_metered_at, a.leased_until
FROM fleet.runner_leases l JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_delayed_renewal_cannot_revive_an_expired_lease_or_affinity() {
    for expired_half in [EXPIRE_LEASE, EXPIRE_AFFINITY] {
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

async fn expire(running: &Running, statement: &'static str) {
    let target = if statement == EXPIRE_LEASE {
        running.lease.as_str()
    } else {
        &running.fleet
    };
    let mut connection = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("expiry injection");
    sqlx::query(statement)
        .bind(target)
        .bind(running.now.as_millis() - 1)
        .execute(&mut *connection)
        .await
        .expect("simulate original expiry");
}

pub(super) async fn snapshot(running: &Running) -> (i64, i64, i64) {
    let mut connection = running
        .fixtures
        .database
        .acquire()
        .await
        .expect("inspect renewal");
    sqlx::query_as(READ)
        .bind(running.lease.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("read deadline and meter")
}

pub(super) fn free_meter() -> Meter {
    Meter {
        cumulative: Cumulative::default(),
        rates: SliceRates {
            run_nanos_per_sec: 0,
            input_nanos_per_mtok: 0,
            cached_input_nanos_per_mtok: 0,
            output_nanos_per_mtok: 0,
        },
    }
}
