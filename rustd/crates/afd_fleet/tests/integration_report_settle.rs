//! §3 against live datastores — the fence, the flip, and the dedup.
//!
//! Dimensions 3.1, 3.2 and 3.3. These drive [`Leases`] rather than the whole
//! `Plane`, and that is the point: what §3 claims is ROW parity, and the rows
//! are decided entirely by one statement. Building a `Plane` here would add a
//! vault, a provider resolver and a connector registry to a test whose subject
//! is a `WITH` clause, and every one of them is proven elsewhere.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_report_reads.rs"]
mod report_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_report_seed.rs"]
mod report_seed;

use afd_fleet::lease::Settled;
use afd_fleet::sql;

use self::report_seed::{DEEP_POOL, SLICE_MS, SLICE_NANOS, held, run_fee_meter};

/// Dimension 3.1 — the report's writes land, and every row says what it should.
///
/// Six writes ride one statement and this asserts all of them, because the
/// statement is all-or-nothing: any single arm that silently matched no row
/// would leave the other five looking correct. The lease flip is the one an
/// eyeball would check; the affinity cursor and the lifetime tally are the two
/// that have failed silently in this family before.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_writes_row_parity() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    // One second of runtime, at one nano per millisecond.
    let settled_at = held.now.saturating_add_millis(SLICE_MS);

    let outcome = held
        .leases
        .claim_and_settle(lease, &held.runner, run_fee_meter(), true, settled_at)
        .await
        .expect("the settle must reach the datastore");
    let Settled::Claimed(charged) = outcome else {
        unreachable!("the only holder of this fleet cannot be fenced out")
    };
    assert_eq!(
        charged.as_i64(),
        SLICE_NANOS,
        "a second of runtime at one nano per millisecond is a thousand nanos"
    );

    assert_eq!(
        held.fixtures.lease_column(lease, "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the claim flips the lease out of active, which is what stops a second report"
    );
    assert_eq!(
        held.fixtures.lease_column(lease, "last_metered_at").await,
        Some(settled_at.as_millis().to_string()),
        "the lease cursor advances to the settle instant, so a replay measures zero elapsed"
    );
    assert_eq!(
        held.fixtures
            .affinity_column(&held.fleet, "last_metered_at")
            .await,
        Some(settled_at.as_millis().to_string()),
        "the SLOT cursor advances too — it is the one a reclaim inherits, and the \
         one the deltas are actually measured from"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - SLICE_NANOS),
        "the wallet is drawn down by exactly what the settle reported charging"
    );
    assert_eq!(
        held.fixtures
            .ledger_column(
                &held.event_id,
                afd_billing::sql::charge::STAGE,
                "credit_deducted_nanos"
            )
            .await,
        Some(SLICE_NANOS.to_string()),
        "the stage row records the same amount the wallet lost"
    );
    assert_eq!(
        held.fixtures
            .ledger_column(&held.event_id, afd_billing::sql::charge::STAGE, "wall_ms")
            .await,
        Some(SLICE_MS.to_string()),
        "the stage row carries the span it charged over, which the budget drain apportions on"
    );
    assert_eq!(
        held.fixtures
            .counter_column(held.runner.as_str(), "succeeded")
            .await,
        Some("1".to_owned()),
        "a clean run bumps the succeeded column and not the other one"
    );
    assert_eq!(
        held.fixtures
            .counter_column(held.runner.as_str(), "failed")
            .await,
        Some("0".to_owned()),
        "the tally picks ONE column; both moving would mean the CASE arms disagree"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 3.2 — a superseded holder is refused and mutates nothing.
///
/// The strong half of this test is the second assertion set, not the first.
/// Refusing the stale writer is easy; refusing it having written NO row is the
/// property, and it holds only because every write arm is gated `FROM guard`.
/// A statement that flipped the lease before checking the fence would pass an
/// assertion on the return value and corrupt the current holder's run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_stale_fence_rejected() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();

    // The holder dies and its work is taken back, which bumps the fleet's
    // fencing sequence past the token the first lease carries.
    let lapsed = held
        .now
        .saturating_add_millis(afd_core::timing::LEASE_TTL_MS + 1);
    let reclaimed = held
        .leases
        .select(&held.spare, lapsed)
        .await
        .expect("the reclaim pass must not fault")
        .expect("a lapsed claim is winnable");
    assert!(
        reclaimed.fence > held.fence,
        "the reclaim must outrank the holder it displaced, or this test proves nothing"
    );

    let before = held.fixtures.balance(&held.tenant).await;
    let outcome = held
        .leases
        .claim_and_settle(lease, &held.runner, run_fee_meter(), true, lapsed)
        .await
        .expect("a fenced settle is an answer, not a fault");
    assert_eq!(
        outcome,
        Settled::Fenced,
        "the displaced holder must not win the report; the current holder's result wins"
    );

    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        before,
        "a fenced report charges nothing — the wallet arm is gated on the same guard"
    );
    assert_eq!(
        held.fixtures
            .ledger_column(
                &held.event_id,
                afd_billing::sql::charge::STAGE,
                "credit_deducted_nanos"
            )
            .await,
        None,
        "a fenced report writes no stage row at all, not a zero-valued one"
    );
    // BOTH arms, and both `Some("0")` rather than `None`. The counter ROW
    // already exists: `sql::lease::INSERT_LEASE_WITH_EVENT` writes it with
    // `acquired = 1` when the lease is issued, so absence was never the shape
    // this could take and asserting `None` tested the fixture, not the guard.
    // What the guard actually controls is whether either OUTCOME arm ticks —
    // `tally` selects `FROM claim`, and a fenced report claims no rows — so
    // pinning both to zero is what a wrongly-gated tally would break, on
    // whichever arm the `succeeded` flag steered it to.
    assert_eq!(
        held.fixtures
            .counter_column(held.runner.as_str(), "succeeded")
            .await,
        Some("0".to_owned()),
        "the tally is gated FROM claim, so a report that claimed nothing counts nothing"
    );
    assert_eq!(
        held.fixtures
            .counter_column(held.runner.as_str(), "failed")
            .await,
        Some("0".to_owned()),
        "and it counts nothing on the failure arm either, not merely the one this report named"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 3.3 — a replayed report leaves the ledger at two rows.
///
/// Two, not one: the receive row §2 wrote and the stage row §3 accumulates
/// into. The `ON CONFLICT (event_id, charge_type)` arm is what holds it there,
/// and the second charge is approximately nothing because the first advanced
/// the cursors the deltas are measured from.
///
/// The replay is also FENCED — the lease is no longer `active` after the first
/// claim — so this proves both guards at once: the second report cannot claim,
/// and cannot charge.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_dedup_idempotent() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    let settled_at = held.now.saturating_add_millis(SLICE_MS);

    let first = held
        .leases
        .claim_and_settle(lease, &held.runner, run_fee_meter(), true, settled_at)
        .await
        .expect("the first settle must reach the datastore");
    assert!(
        matches!(first, Settled::Claimed(_)),
        "the first report wins the fence"
    );
    let after_first = held.fixtures.balance(&held.tenant).await;
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        2,
        "one receive row and one stage row: the two-rows-per-event invariant"
    );

    let replay = held
        .leases
        .claim_and_settle(lease, &held.runner, run_fee_meter(), true, settled_at)
        .await
        .expect("the replay must reach the datastore");
    assert_eq!(
        replay,
        Settled::Fenced,
        "the lease is no longer active, so the replay claims nothing"
    );
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        2,
        "a replayed report adds no row — the ledger stays at two however often it is re-sent"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        after_first,
        "and charges nothing the second time"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
