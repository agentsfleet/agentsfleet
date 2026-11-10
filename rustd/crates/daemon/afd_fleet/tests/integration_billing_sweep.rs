//! The billing relay settles what the lease path logged, and bills the last
//! slice of a lease that ended unreported.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::usage::{DAEMON_SEQ_BASE, DECISION_SEQ_BASE, UsageLine};
use afd_billing::biller::{Account, Attribution, Biller, Settlement, WorkKey};
use afd_billing::ledger::{LedgerBiller, log_lines};
use afd_billing::price::RatedLine;
use afd_billing::{Nanos, Posture};
use afd_fleet::lease::billing_sweep::{BillingSwept, GRACE, sweep};
use afd_fleet::lease::{Renewed, sql::meter::Metering};
use futures_util::FutureExt as _;

use crate::report_seed::{DEEP_POOL, Held, held_in};
use crate::support::Fixtures;

/// What the unsettled line costs.
const LINE: Nanos = Nanos::from_i64(40_000);

/// The runtime the orphaned line measured.
const RUNTIME_MS: u32 = 400;

/// Ages both of a row's stamps past the sweeper's grace.
const AGED: &str = "UPDATE {table} SET created_at = created_at - $2::interval, \
                    updated_at = updated_at - $2::interval WHERE {key} = $1";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_relay_settles_orphaned_lines() {
    // Its own database: a pass sweeps every unsettled line it can see, so the
    // counts below are exact only where this test's rows are the only ones.
    let held = held_in(Fixtures::create_isolated().await).await;
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(af_core::clock::SystemClock),
    );
    orphan(&held).await;

    let swept = sweep(database, &biller, &Entropy::new(), held.now, None)
        .await
        .expect("the pass reads");
    assert_eq!(
        swept,
        BillingSwept {
            resettled: 1,
            ..BillingSwept::default()
        }
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - LINE.as_i64()),
        "the orphaned line is charged once"
    );
    let again = sweep(database, &biller, &Entropy::new(), held.now, None)
        .await
        .expect("the pass reads");
    assert_eq!(
        again,
        BillingSwept::default(),
        "a second pass finds nothing left"
    );

    held.fixtures.cleanup().await;
}

/// A lease that ended unreported owes the run time between its last renewal
/// and the deadline it was granted (spec Dimension 9.5). The sweeper logs it as
/// one runtime line in the daemon's range, settles it, and a second pass finds
/// nothing more to bill.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_fleet_expiry_bills_its_last_slice() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(
        Duration::from_secs(45),
        expiry_after_decision(&held),
    ))
    .catch_unwind()
    .await;
    tokio::time::timeout(Duration::from_secs(10), held.fixtures.cleanup())
        .await
        .expect("owned fixture cleanup remains bounded after failure");
    match outcome {
        Ok(result) => result.expect("renewal and expiry scenario is bounded"),
        Err(failure) => std::panic::resume_unwind(failure),
    }
}

async fn expiry_after_decision(held: &Held) {
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(af_core::clock::SystemClock),
    );
    // Seed one known decision charge through the real ledger. Physical-call
    // reservation is a separate component; this tests its reserved namespace.
    let mut decision = settlement(held).await;
    "typesafe".clone_into(&mut decision.provider);
    "jev-latest".clone_into(&mut decision.model);
    decision.lines = vec![RatedLine {
        usage: UsageLine {
            seq: DECISION_SEQ_BASE,
            at: held.now,
            input_tokens: 1_000,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 0,
            runtime_ms: 0,
        },
        price: None,
        charged: DECISION_CHARGE,
        unpriced_tokens: 0,
    }];
    biller
        .settle(&decision)
        .await
        .expect("known decision charge");
    let first_ms = renew_runtime(held, &biller).await;
    assert_eq!(
        runtime_lines(held).await,
        [(i64::from(DAEMON_SEQ_BASE), first_ms)],
        "renewal counts runtime only, despite the existing decision receipt"
    );
    let before = DEEP_POOL - DECISION_CHARGE.as_i64() - crate::report_seed::run_fee(first_ms);
    assert_eq!(held.fixtures.balance(&held.tenant).await, Some(before));
    let tail_ms = abandon(held).await;
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(before),
        "abandonment preserves previously charged work"
    );

    let swept = sweep(database, &biller, &Entropy::new(), held.now, None)
        .await
        .expect("the pass reads");
    assert_eq!(
        swept,
        BillingSwept {
            expired_billed: 1,
            ..BillingSwept::default()
        }
    );
    assert_eq!(
        runtime_lines(held).await,
        [
            (i64::from(DAEMON_SEQ_BASE), first_ms),
            (i64::from(DAEMON_SEQ_BASE) + 1, tail_ms)
        ],
        "expiry uses the next runtime sequence without counting decision usage"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(before - crate::report_seed::run_fee(tail_ms)),
        "the last slice is charged once"
    );

    let again = sweep(database, &biller, &Entropy::new(), held.now, None)
        .await
        .expect("the pass reads");
    assert_eq!(
        again,
        BillingSwept::default(),
        "the cursor reached the deadline"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(before - crate::report_seed::run_fee(tail_ms))
    );
}

/// Recorded normalized usage, independent of the provider-rate integration.
const DECISION_CHARGE: Nanos = Nanos::from_i64(42_000);

async fn renew_runtime(held: &Held, biller: &LedgerBiller) -> i64 {
    held.run_a_slice().await;
    let renewed = held
        .leases
        .extend(
            Metering {
                lease_id: &held.issued.lease_id,
                runner_id: &held.runner,
                requests: &[],
            },
            &held.lease_key,
            &held.pricing,
            held.now,
        )
        .await
        .expect("current lease renews");
    let settlement = match renewed {
        Renewed::Extended { settlement, .. } => settlement,
        _ => None,
    }
    .expect("renewal measured positive runtime");
    assert_eq!(settlement.lines.len(), 1, "coding request list was empty");
    let line = settlement.lines.first().expect("one runtime line");
    assert!(line.usage.runtime_ms >= 1_000);
    let millis = i64::from(line.usage.runtime_ms);
    biller
        .settle(&settlement)
        .await
        .expect("first runtime charge");
    millis
}

/// The expiry sweep and a late report race on one abandoned lease: whichever
/// runs second finds the cursor at the deadline, so the tail is billed once,
/// never twice and never not at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_expiry_racing_a_late_report_bills_the_tail_once() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let database = &held.fixtures.database;
    let biller = LedgerBiller::new(
        database.clone(),
        Entropy::new(),
        Arc::new(af_core::clock::SystemClock),
    );
    let tail_ms = abandon(&held).await;

    let entropy = Entropy::new();
    let swept = sweep(database, &biller, &entropy, held.now, None);
    let reported = held.fixtures.settle_alone(
        &held.leases,
        &held.issued.lease_id,
        &held.runner,
        &[],
        true,
        held.now,
    );
    let (swept, _reported) = tokio::join!(swept, reported);
    swept.expect("the pass reads");

    let charged_ms: i64 = runtime_lines(&held).await.iter().map(|(_, ms)| ms).sum();
    assert_eq!(
        charged_ms, tail_ms,
        "the tail is billed exactly once between the two"
    );

    held.fixtures.cleanup().await;
}

/// Leaves `held`'s lease active but abandoned: its deadline and its cursor
/// both pushed back past the sweeper's grace, with three seconds of run time
/// between them that no renewal billed. Answers that tail in milliseconds.
pub(crate) async fn abandon(held: &Held) -> i64 {
    let lease = &held.issued.lease_id;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "UPDATE runner.leases SET lease_expires_at = now() - $2::interval, \
         last_metered_at = now() - $2::interval - $3::interval WHERE id = $1",
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
const TAIL: Duration = Duration::from_secs(3);

/// `held`'s lease's runtime lines, as `(seq, runtime_ms)`.
async fn runtime_lines(held: &Held) -> Vec<(i64, i64)> {
    sqlx::query_as(
        "SELECT seq, runtime_ms FROM billing.usage_lines \
         WHERE work_id = $1 AND seq >= $2 AND seq < $3 ORDER BY seq",
    )
    .bind(&held.lease_key)
    .bind(i64::from(DAEMON_SEQ_BASE))
    .bind(i64::from(DECISION_SEQ_BASE))
    .fetch_all(
        &mut *held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection"),
    )
    .await
    .expect("the runtime lines read")
}

/// The state a crash leaves: one line logged, then the process died before
/// any relay pass settled it, and the lease expired unreported, all longer ago
/// than the grace.
pub(crate) async fn orphan(held: &Held) {
    let database = &held.fixtures.database;
    let settlement = settlement(held).await;
    log_lines(
        &mut *database.acquire().await.expect("a pooled connection"),
        &Entropy::new(),
        held.now,
        &settlement,
    )
    .await
    .expect("the line is logged");

    let mut connection = database.acquire().await.expect("a pooled connection");
    sqlx::query("UPDATE runner.leases SET status = $2 WHERE id = $1")
        .bind(&held.lease_key)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
        .execute(&mut *connection)
        .await
        .expect("the lease ends unreported");
    for (table, key) in [("billing.usage_lines", "work_id"), ("runner.leases", "id")] {
        sqlx::query(sqlx::AssertSqlSafe(
            AGED.replace("{table}", table).replace("{key}", key),
        ))
        .bind(&held.lease_key)
        .bind(GRACE.saturating_mul(2))
        .execute(&mut *connection)
        .await
        .expect("the rows age past the grace");
    }
}

/// One runtime-only line of `held`'s lease, attributed to its fleet.
pub(crate) async fn settlement(held: &Held) -> Settlement {
    let workspace: Uuid7 =
        sqlx::query_scalar("SELECT workspace_id FROM fleet.fleets WHERE id = $1")
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
            tenant: held.tenant.clone(),
            workspace,
        },
        attribution: Attribution::Fleet {
            fleet: held.fleet.clone(),
            fleet_name: held.fleet.clone().into(),
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
