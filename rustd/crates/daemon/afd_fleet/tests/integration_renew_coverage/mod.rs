//! Renewal coverage decisions that need live rows but no broad lane orchestration.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::error_code;
use af_wire::report::RenewRequest;

use crate::report_seed::{DEEP_POOL, Held, held};

/// The status an archive leaves on the fleet row.
///
/// `Runs::installed` answers `Ok(None)` for it, as for every status that is
/// not `active`. The renewal of a run already in flight must not.
const FLEET_ARCHIVED: &str = "archived";

async fn remove_wallet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM billing.wallets WHERE tenant_id = $1")
        .bind(&held.tenant)
        .execute(&mut *connection)
        .await
        .expect("the wallet fixture is removed");
}

async fn archive_fleet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1")
        .bind(&held.fleet)
        .bind(FLEET_ARCHIVED)
        .execute(&mut *connection)
        .await
        .expect("the live fleet is archived");
}

// Value: protects=a payer whose wallet row is gone cannot keep a run alive,
// as it cannot start one; fails_when=a missing wallet renews as an unlimited
// one; seam=`renew` over live Postgres, the wallet row deleted mid-run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_run_whose_wallet_row_is_gone_is_not_renewed() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let run_id = &held.issued.run_id;

    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    plane
        .renew(&held.runner, run_id, RenewRequest::default(), held.now)
        .await
        .expect("a funded run renews");
    remove_wallet(&held).await;
    let refused = plane
        .renew(&held.runner, run_id, RenewRequest::default(), held.now)
        .await
        .expect_err("a payer with no wallet funds nothing");
    assert_eq!(refused.code(), error_code::RUN_RENEWAL_NO_CREDITS);

    drop(plane);
    held.fixtures.cleanup().await;
}

/// An archive never cancels a run that still has room: the run in flight renews.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_archived_mid_run_with_room_still_renews() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let run_id = &held.issued.run_id;

    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    archive_fleet(&held).await;
    plane
        .renew(&held.runner, run_id, RenewRequest::default(), held.now)
        .await
        .expect("an archived fleet's run with room left keeps renewing");

    drop(plane);
    held.fixtures.cleanup().await;
}

mod batch;
mod catalogue;
mod refusals;
