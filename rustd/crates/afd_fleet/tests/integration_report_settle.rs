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

use crate::report_seed;
use afd_fleet::lease::Settled;
use afd_fleet::lease::sql;

use self::report_seed::{DEEP_POOL, SLICE_NANOS, held, run_fee};
use crate::rows::{LAPSE, Row};

/// The line number a lease's first runtime line takes: the daemon's base.
const RUNTIME_LINE: &str = "2147483648";

/// Dimension 3.1 — the report's writes land, and every row says what it should.
///
/// Six writes ride one statement and this asserts all of them, because the
/// statement is all-or-nothing: any single arm that silently matched no row
/// would leave the other five looking correct. The lease flip is the one an
/// eyeball would check; the slot cursor and the lifetime tally are the two
/// that have failed silently in this family before.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_writes_row_parity() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    // At least one second of runtime, at one nano per millisecond.
    let from = held.run_a_slice().await;
    let settled_at = held.now;

    let outcome = held
        .fixtures
        .settle_alone(&held.leases, lease, &held.runner, &[], true, settled_at)
        .await;
    let Settled::Claimed(settlement) = outcome else {
        unreachable!("the only holder of this fleet cannot be fenced out")
    };
    let wall_ms = held.metered_from(from).await;
    let slice = run_fee(wall_ms);
    assert!(
        slice >= SLICE_NANOS,
        "the aged meter charges at least a slice: {slice}"
    );
    let charged: i64 = settlement
        .iter()
        .flat_map(|settled| settled.lines.iter())
        .map(|line| line.charged.as_i64())
        .sum();
    assert_eq!(
        charged, slice,
        "the line is the meter's span at the run fee, rounded up to the nano"
    );

    assert_eq!(
        held.fixtures.text(Row::Lease(lease), "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the claim flips the lease out of active, which is what stops a second report"
    );
    assert!(
        held.fixtures.metered_since(&held.fleet).await <= held.fixtures.database_now().await,
        "the lease's cursor advances to the settle's own instant, never past it"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL),
        "the claim logs usage and moves no money; the biller charges it after commit"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Line(&held.event_id, RUNTIME_LINE), "charged_nanos")
            .await,
        Some(slice.to_string()),
        "the usage line records what the slice cost"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Line(&held.event_id, RUNTIME_LINE), "runtime_ms")
            .await,
        Some(wall_ms.to_string()),
        "the usage line carries the span it measured"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Counters(held.runner.as_str()), "succeeded_count")
            .await,
        Some("1".to_owned()),
        "a clean run bumps the succeeded column and not the other one"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Counters(held.runner.as_str()), "failed_count")
            .await,
        Some("0".to_owned()),
        "the tally picks ONE column; both moving would mean the CASE arms disagree"
    );

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
    held.fixtures
        .age(Row::Slot(&held.fleet), "lease_expires_at", LAPSE)
        .await;
    let reclaimed = crate::seed::select_fleet(&held.leases, &held.spare, &held.fleet)
        .await
        .expect("a lapsed claim is winnable");
    assert!(
        reclaimed.fence > held.fence,
        "the reclaim must outrank the holder it displaced, or this test proves nothing"
    );

    let before = held.fixtures.balance(&held.tenant).await;
    let outcome = held
        .fixtures
        .settle_alone(&held.leases, lease, &held.runner, &[], true, held.now)
        .await;
    assert_eq!(
        outcome,
        Settled::Fenced,
        "the displaced holder must not win the report; the current holder's result wins"
    );

    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        before,
        "a fenced report charges nothing"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Line(&held.event_id, RUNTIME_LINE), "charged_nanos")
            .await,
        None,
        "a fenced report logs no usage line at all, not a zero-valued one"
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
            .text(Row::Counters(held.runner.as_str()), "succeeded_count")
            .await,
        Some("0".to_owned()),
        "the tally is gated FROM claim, so a report that claimed nothing counts nothing"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Counters(held.runner.as_str()), "failed_count")
            .await,
        Some("0".to_owned()),
        "and it counts nothing on the failure arm either, not merely the one this report named"
    );

    held.fixtures.cleanup().await;
}

/// Dimension 3.3 — a replayed report leaves the ledger at two rows.
///
/// Two, not one: the receive row §2 wrote and the stage row §3 accumulates
/// into. The `ON CONFLICT (event_id, charge_type, fleet_id)` arm holds it there,
/// and the second charge is approximately nothing because the first advanced
/// the cursors the deltas are measured from.
///
/// The replay claims nothing — the lease is no longer `active` after the first
/// claim — so this proves both guards at once: the second report cannot claim,
/// and cannot charge. Which of the two empty claims it is matters as much as
/// that it is empty: this runner settled the lease, so the statement answers
/// `AlreadySettled` and the verb above it hands the runner the outcome it
/// already earned instead of a refusal it can never retry past.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_dedup_idempotent() {
    let held = held().await;
    let lease = held.issued.lease_id.as_str();
    let settled_at = held.now;

    let first = held
        .fixtures
        .settle_alone(&held.leases, lease, &held.runner, &[], true, settled_at)
        .await;
    assert!(
        matches!(first, Settled::Claimed(_)),
        "the first report wins the fence"
    );
    let after_first = held.fixtures.balance(&held.tenant).await;
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        1,
        "one usage line for the final slice"
    );

    let replay = held
        .fixtures
        .settle_alone(&held.leases, lease, &held.runner, &[], true, settled_at)
        .await;
    assert_eq!(
        replay,
        Settled::AlreadySettled,
        "the lease is no longer active, so the replay claims nothing — and it is \
         THIS runner's settled lease, not a fleet taken from it"
    );
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        1,
        "a replayed report adds no line however often it is re-sent"
    );
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        after_first,
        "and charges nothing the second time"
    );

    held.fixtures.cleanup().await;
}
