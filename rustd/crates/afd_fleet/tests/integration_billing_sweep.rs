//! The billing sweeper finishes what the lease path could not: it settles a
//! usage line that committed without its settlement, and releases the hold of
//! a lease that ended without releasing it.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_billing::biller::{
    Account, Attribution, Biller as _, Hold, Reservation, Settlement, WorkKey,
};
use afd_billing::ledger::{LedgerBiller, log_lines};
use afd_billing::price::RatedLine;
use afd_billing::{Nanos, Posture};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::billing_sweep::{BillingSwept, GRACE, sweep};
use afd_wire::usage::{DAEMON_SEQ_BASE, UsageLine};

use crate::queue;
use crate::report_seed::{DEEP_POOL, Held, held_in};
use crate::support::Fixtures;

/// What the unsettled line costs.
const LINE: Nanos = Nanos::from_i64(40_000);

/// The runtime the orphaned line measured.
const RUNTIME_MS: u32 = 400;

/// The hold the finished lease left open.
const HELD: Nanos = Nanos::from_i64(90_000);

/// Ages both of a row's stamps past the sweeper's grace.
const AGED: &str = "UPDATE {table} SET created_at = created_at - $2::interval, \
                    updated_at = updated_at - $2::interval WHERE {key} = $1::uuid";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_sweeper_settles_orphaned_lines_and_releases_finished_holds() {
    // Its own database: a pass sweeps every unsettled line it can see, so the
    // counts below are exact only where this test's rows are the only ones.
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(afd_core::clock::SystemClock),
    );
    orphan(&held, &biller).await;

    let swept = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(
        swept,
        BillingSwept {
            resettled: 1,
            released: 1,
            ..BillingSwept::default()
        }
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - LINE.as_i64()),
        "the orphaned line is charged once"
    );
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.holds WHERE work_id = $1::uuid AND released_at IS NULL",
    )
    .bind(held.lease_key.as_str())
    .fetch_one(&mut *database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the holds count");
    assert_eq!(
        open, 0,
        "the finished lease's hold no longer counts against the balance"
    );

    let again = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(
        again,
        BillingSwept::default(),
        "a second pass finds nothing left"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A lease that ended unreported owes the run time between its last renewal
/// and the deadline it was granted (spec Dimension 9.5). The sweeper logs it as
/// one runtime line in the daemon's range, settles it, and a second pass finds
/// nothing more to bill.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_fleet_expiry_bills_its_last_slice() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(afd_core::clock::SystemClock),
    );
    let tail_ms = abandon(&held).await;

    let swept = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(swept.expired_billed, 1, "{swept:?}");
    assert_eq!(
        runtime_lines(&held).await,
        [(i64::from(DAEMON_SEQ_BASE), tail_ms)],
        "one runtime line, in the daemon's range, for the run time since the last renewal"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - crate::report_seed::run_fee(tail_ms)),
        "the last slice is charged once"
    );

    let again = sweep(database, &biller, &Entropy::new(), held.now)
        .await
        .expect("the pass reads");
    assert_eq!(again.expired_billed, 0, "the cursor reached the deadline");

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// The expiry sweep and a late report race on one abandoned lease: whichever
/// runs second finds the cursor at the deadline, so the tail is billed once,
/// never twice and never not at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_expiry_racing_a_late_report_bills_the_tail_once() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(afd_core::clock::SystemClock),
    );
    let tail_ms = abandon(&held).await;

    let entropy = Entropy::new();
    let swept = sweep(database, &biller, &entropy, held.now);
    let reported = held.fixtures.settle_alone(
        &held.leases,
        held.issued.lease_id.as_str(),
        &held.runner,
        &[],
        true,
        held.now,
    );
    let (swept, _reported) = tokio::join!(swept, reported);
    swept.expect("the pass reads");

    let charged_ms: i64 = runtime_lines(&held).await.iter().map(|(_, ms)| ms).sum();
    assert_eq!(charged_ms, tail_ms, "the tail is billed exactly once between the two");

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Leaves `held`'s lease active but abandoned: its deadline and its cursor
/// both pushed back past the sweeper's grace, with three seconds of run time
/// between them that no renewal billed. Answers that tail in milliseconds.
async fn abandon(held: &Held) -> i64 {
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    let lease = held.issued.lease_id.as_str();
    let mut connection = held.fixtures.database.acquire().await.expect("a pooled connection");
    sqlx::query(
        "UPDATE runner.leases SET lease_expires_at = now() - $2::interval, \
         last_metered_at = now() - $2::interval - $3::interval WHERE id = $1::uuid",
    )
    .bind(lease)
    .bind(GRACE.saturating_mul(2))
    .bind(TAIL)
    .execute(&mut *connection)
    .await
    .expect("the lease is abandoned");
    i64::try_from(TAIL.as_millis()).expect("three seconds fit")
}

/// The run time an abandoned lease's last renewal left unbilled.
const TAIL: std::time::Duration = std::time::Duration::from_secs(3);

/// `held`'s lease's runtime lines, as `(seq, runtime_ms)`.
async fn runtime_lines(held: &Held) -> Vec<(i64, i64)> {
    sqlx::query_as(
        "SELECT seq, runtime_ms FROM billing.usage_lines \
         WHERE work_id = $1::uuid AND seq >= $2 ORDER BY seq",
    )
    .bind(held.lease_key.as_str())
    .bind(i64::from(DAEMON_SEQ_BASE))
    .fetch_all(&mut *held.fixtures.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the runtime lines read")
}

/// The state a crash leaves: the lease's hold placed and one line logged, then
/// the process died — no settle, no release — and the lease expired
/// unreported, all longer ago than the sweeper's grace.
async fn orphan(held: &Held, biller: &LedgerBiller) {
    let database = &held.fixtures.database;
    let settlement = settlement(held).await;
    let placed = biller
        .reserve(&Reservation {
            work: settlement.work.clone(),
            payer: settlement.payer.clone(),
            ceiling: HELD,
        })
        .await
        .expect("the hold is placed");
    assert!(matches!(placed, Hold::Held { .. }), "{placed:?}");
    log_lines(
        &mut *database.acquire().await.expect("a pooled connection"),
        &Entropy::new(),
        held.now,
        &settlement,
    )
    .await
    .expect("the line is logged");

    let mut connection = database.acquire().await.expect("a pooled connection");
    sqlx::query("UPDATE runner.leases SET status = $2 WHERE id = $1::uuid")
        .bind(held.lease_key.as_str())
        .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
        .execute(&mut *connection)
        .await
        .expect("the lease ends unreported");
    for (table, key) in [
        ("billing.usage_lines", "work_id"),
        ("billing.holds", "work_id"),
        ("runner.leases", "id"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(
            AGED.replace("{table}", table).replace("{key}", key),
        ))
        .bind(held.lease_key.as_str())
        .bind(GRACE.saturating_mul(2))
        .execute(&mut *connection)
        .await
        .expect("the rows age past the grace");
    }
}

/// One runtime-only line of `held`'s lease, attributed to its fleet.
async fn settlement(held: &Held) -> Settlement {
    let workspace: String =
        sqlx::query_scalar("SELECT workspace_id::text FROM fleet.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(
                &mut *held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("a pooled connection"),
            )
            .await
            .expect("the fixture fleet is readable");
    Settlement {
        work: WorkKey::FleetLease(held.lease_key.clone()),
        payer: Account {
            tenant: Uuid7::parse(&held.tenant).expect("a v7 tenant"),
            workspace: Uuid7::parse(&workspace).expect("a v7 workspace"),
        },
        attribution: Attribution::Fleet {
            fleet: Uuid7::parse(&held.fleet).expect("a v7 fleet"),
            fleet_name: held.fleet.clone(),
            event_id: held.event_id.clone(),
        },
        provider: crate::seed::PROVIDER.to_owned(),
        model: crate::seed::MODEL.to_owned(),
        posture: Posture::Platform,
        lines: vec![RatedLine {
            usage: UsageLine {
                seq: 0,
                at: held.now,
                input_tokens: 0,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
                output_tokens: 0,
                runtime_ms: RUNTIME_MS,
            },
            price: None,
            charged: LINE,
            unpriced_tokens: 0,
        }],
    }
}
