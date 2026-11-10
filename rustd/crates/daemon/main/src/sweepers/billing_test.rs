//! Billing relay progress and cancellation scenarios.

use af_core::error_code;
use af_core::test_util::trace::Capture;
use afd_db::test_util::unreachable_db;

use afd_fleet::lease::billing_sweep::BillingSwept;

use std::cell::Cell;

use tokio_util::sync::CancellationToken;

use super::{
    DRAIN_PASSES, FAILED, LAP_TICKS, REFUSED, drain, drains, lap, log_moved, restarts, sweep_leases,
};

/// The read position survives every tick of a lap and is cleared on the
/// lap's last tick, so the next pass starts at the oldest line.
#[test]
fn a_lap_clears_the_read_position_on_its_last_tick() {
    let resume = std::sync::Mutex::new(Some(7_u8));
    let mut ticks = 0;
    for _ in 1..LAP_TICKS {
        lap(&mut ticks, &resume);
    }
    assert_eq!(
        resume.lock().map(|at| *at).ok(),
        Some(Some(7)),
        "kept within the lap"
    );
    lap(&mut ticks, &resume);
    assert_eq!(
        resume.lock().map(|at| *at).ok(),
        Some(None),
        "cleared at the lap"
    );
}

/// The line read goes back to the oldest line once a lap and only then, so
/// refused lines are retried and progress through newer lines is kept.
#[test]
fn the_read_restarts_at_the_oldest_line_once_a_lap() {
    let restarted: Vec<u64> = (1..=LAP_TICKS * 2).filter(|tick| restarts(*tick)).collect();
    assert_eq!(restarted, [LAP_TICKS, LAP_TICKS * 2]);
}

/// Passes reporting more waiting, then nothing: one tick runs all three
/// back to back rather than one per tick.
#[tokio::test]
async fn a_tick_runs_passes_until_one_leaves_nothing_waiting() {
    let answers = [true, true, false];
    let next = Cell::new(0);
    let ran = drain(
        || {
            let answer = answers.get(next.get()).copied().unwrap_or(false);
            next.set(next.get() + 1);
            async move { answer }
        },
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(ran, 3);
}

/// A backlog that never empties stops at the cap and yields to the next
/// tick.
#[tokio::test]
async fn a_tick_stops_at_the_drain_cap() {
    let ran = drain(|| async { true }, &CancellationToken::new()).await;
    assert_eq!(ran, DRAIN_PASSES);
}

#[tokio::test]
async fn a_cancelled_tick_starts_no_ledger_pass() {
    let stop = CancellationToken::new();
    stop.cancel();
    let called = Cell::new(0);
    let ran = drain(
        || {
            called.set(called.get() + 1);
            async { true }
        },
        &stop,
    )
    .await;
    assert_eq!(ran, 0);
    assert_eq!(called.get(), 0);
}

#[tokio::test]
async fn cancellation_during_a_pass_starts_no_following_pass() {
    let stop = CancellationToken::new();
    let called = Cell::new(0);
    let ran = drain(
        || {
            called.set(called.get() + 1);
            stop.cancel();
            async { true }
        },
        &stop,
    )
    .await;
    assert_eq!(ran, 1);
    assert_eq!(called.get(), 1);
}

/// A pass the biller refused part of warns once, under the refusal's code.
#[test]
fn a_refused_pass_warns_with_its_code() {
    let capture = Capture::install();
    log_moved(&BillingSwept {
        settle_failures: 2,
        refused: Some(error_code::INTERNAL_DB_UNAVAILABLE),
        ..BillingSwept::default()
    });
    assert_eq!(
        capture.only(REFUSED).field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
}

/// A pass that read a whole batch runs again at once, settled or refused,
/// so a batch the biller refused never holds newer lines behind it.
#[test]
fn a_full_pass_drains_again_whether_or_not_it_settled() {
    for swept in [
        BillingSwept {
            resettled: 3,
            lines_full: true,
            ..BillingSwept::default()
        },
        BillingSwept {
            settle_failures: 500,
            lines_full: true,
            ..BillingSwept::default()
        },
    ] {
        assert!(drains(&swept), "{swept:?}");
    }
}

/// A pass that read less than a batch left nothing waiting: the tick ends.
#[test]
fn a_partial_pass_waits_for_the_tick() {
    let partial = BillingSwept {
        resettled: 3,
        ..BillingSwept::default()
    };
    assert!(!drains(&partial));
}

/// A lease pass whose datastore is gone says it is waiting, under the
/// outage's code, and returns for the next tick rather than ending the task.
#[tokio::test]
async fn a_lease_pass_with_no_datastore_waits_for_it() {
    let database = unreachable_db();
    let biller = crate::billing::biller(database.clone());
    let capture = Capture::install();

    sweep_leases(&database, biller.as_ref(), &std::sync::Mutex::new(None)).await;

    assert_eq!(
        capture.only(FAILED).field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
}
