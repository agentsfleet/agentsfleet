//! The supervised sweepers' first pass, against the lane: a booted daemon
//! finishes what the request path left behind before it has served anything.
//!
//! A sweeper ticks once at spawn, so seeding before boot is what puts work in
//! front of that first pass. The rows are ones only a sweeper ends: a hold
//! whose fleet lease or setup-chat attempt no longer exists, and a replay
//! receipt whose retry window closed. Asserted on the rows rather than on a
//! log line, because a sibling suite's daemon on the same lane may take the
//! pass first, and the claim is that the rows end, whichever daemon ends them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_db::test_util::{TenantSeed, WalletSeed, mint_id};
use agentsfleetd::serve::boot;
use agentsfleetd::supervisor::Supervisor;
use sqlx::Connection as _;

use crate::e2e::{DATABASE_LANE_KNOB, EPHEMERAL, READY_STREAM, lane};
use crate::e2e_db::daemon_environment;
use crate::support::install_subscriber;

/// The balance the tenant's wallet holds; the holds reserve part of it.
const BALANCE_NANOS: i64 = 1_000_000;

/// What each orphaned hold reserves.
const CEILING_NANOS: i64 = 40_000;

/// The scope the expired receipt was written under.
const SCOPE: &str = "sweeper-fixture";

/// How long the first pass is given; a tick is immediate, so this is slack.
const FIRST_PASS: Duration = Duration::from_secs(15);

/// How often the rows are looked at while the pass runs.
const POLL: Duration = Duration::from_millis(100);

/// An open hold on a work no row names.
const INSERT_HOLD: &str = "INSERT INTO billing.holds \
     (id, work_type, work_id, tenant_id, ceiling_nanos, captured_nanos, created_at, updated_at) \
     VALUES ($1::uuid, $2, $3::uuid, $4::uuid, $5, 0, now(), now())";

/// A receipt two days old, past the one-day replay window.
const INSERT_EXPIRED_RECEIPT: &str = "INSERT INTO authoring.replay_receipts \
     (id, workspace_id, user_id, scope, operation_id, request_digest, resource_id, created_at) \
     VALUES ($1::uuid, $2::uuid, $2::uuid, $3, $1::uuid, 'digest', $2::uuid, \
             now() - interval '2 days')";

/// How many of this tenant's holds are still open, and whether the receipt
/// survives.
const LEFT_BEHIND: &str = "SELECT \
     (SELECT count(*) FROM billing.holds WHERE tenant_id = $1::uuid AND released_at IS NULL), \
     (SELECT count(*) FROM authoring.replay_receipts WHERE id = $2::uuid)";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_the_first_sweep_ends_orphaned_holds_and_expired_receipts() {
    install_subscriber();
    let database_url = lane(DATABASE_LANE_KNOB);
    let (tenant, receipt) = (mint_id(), mint_id());
    let mut seeding = sqlx::PgConnection::connect(&database_url)
        .await
        .expect("the lane's Postgres accepts a connection");
    TenantSeed::new(&tenant)
        .insert(&mut seeding)
        .await
        .expect("the tenant seeds");
    WalletSeed::new(&tenant, BALANCE_NANOS)
        .insert(&mut seeding)
        .await
        .expect("the wallet seeds");
    for work_type in [
        afd_billing::sql::work::FLEET_LEASE,
        afd_billing::sql::work::AUTHORING_ATTEMPT,
    ] {
        sqlx::query(INSERT_HOLD)
            .bind(mint_id())
            .bind(work_type)
            .bind(mint_id())
            .bind(&tenant)
            .bind(CEILING_NANOS)
            .execute(&mut seeding)
            .await
            .expect("the orphaned hold seeds");
    }
    sqlx::query(INSERT_EXPIRED_RECEIPT)
        .bind(&receipt)
        .bind(mint_id())
        .bind(SCOPE)
        .execute(&mut seeding)
        .await
        .expect("the expired receipt seeds");

    // A booted daemon joins the deployment's consumer groups, so it waits its
    // turn beside the scenarios as `e2e` explains.
    let exclusive = READY_STREAM.lock().await;
    let mut supervisor = Supervisor::new();
    let booted = boot(
        &daemon_environment(&database_url, None, &[]),
        EPHEMERAL,
        &mut supervisor,
    )
    .await
    .expect("the lane's Postgres and Dragonfly are up");

    let deadline = tokio::time::Instant::now() + FIRST_PASS;
    let mut left: (i64, i64) = (-1, -1);
    while left != (0, 0) && tokio::time::Instant::now() < deadline {
        left = sqlx::query_as(LEFT_BEHIND)
            .bind(&tenant)
            .bind(&receipt)
            .fetch_one(&mut seeding)
            .await
            .expect("the leftovers count");
        tokio::time::sleep(POLL).await;
    }
    supervisor.shutdown().await;
    drop(booted);
    drop(exclusive);

    assert_eq!(
        left,
        (0, 0),
        "the first pass releases both orphaned holds and deletes the expired receipt"
    );
}
