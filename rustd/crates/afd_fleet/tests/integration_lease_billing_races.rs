//! Two writers on one lease's usage, racing.
//!
//! A lease's line numbers come from the lines it has already logged, read under
//! the lease's row lock, so every pass that could add a line waits on that lock
//! and no two take the same number. These run the passes concurrently on
//! separate connections and check the outcome holds in every interleaving the
//! scheduler picks.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Cumulative;
use afd_fleet::lease::sql::meter::Metering;
use afd_fleet::lease::{Renewed, Settled};

use crate::queue;
use crate::report_seed::held;

/// How many times each race runs, so one lucky interleaving cannot pass it.
const ROUNDS: usize = 5;

/// The usage lines `lease` has logged, by line number.
async fn line_numbers(fixtures: &crate::support::Fixtures, lease: &str) -> Vec<i64> {
    sqlx::query_scalar("SELECT seq FROM billing.usage_lines WHERE work_id = $1::uuid ORDER BY seq")
        .bind(lease)
        .fetch_all(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the lines read")
}

/// A runner re-sends its report while the first is still in flight: exactly
/// one claims the lease and logs the final line, the other is told the lease
/// is already settled, and the lease has one final line, not two.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_reports_of_one_lease_log_one_final_line() {
    for _round in 0..ROUNDS {
        let held = held().await;
        held.run_a_slice().await;
        let lease = held.issued.lease_id.as_str();
        let report = || {
            held.fixtures.settle_alone(
                &held.leases,
                lease,
                &held.runner,
                Cumulative::default(),
                true,
                held.now,
            )
        };
        let (first, second) = tokio::join!(report(), report());

        let claimed = [&first, &second]
            .into_iter()
            .filter(|settled| matches!(settled, Settled::Claimed(_)))
            .count();
        assert_eq!(claimed, 1, "one report wins: {first:?} / {second:?}");
        assert!(
            [&first, &second].contains(&&Settled::AlreadySettled),
            "the other is told this runner already settled it"
        );
        assert_eq!(line_numbers(&held.fixtures, lease).await, [0]);

        queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
        held.fixtures.cleanup().await;
    }
}

/// A renewal and the report race on one lease: whichever runs second numbers
/// its line after the first's, so the lease never holds two lines under one
/// number and never loses one to the `(work, seq)` guard. The second pass may
/// measure nothing, and log no line, when the two land within a millisecond.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_racing_the_report_numbers_its_line_apart() {
    for _round in 0..ROUNDS {
        let held = held().await;
        held.run_a_slice().await;
        let lease = held.issued.lease_id.as_str();
        let renew = held.leases.extend(
            Metering {
                lease_id: lease,
                runner_id: &held.runner,
                cumulative: Cumulative::default(),
            },
            &held.lease_key,
            &held.pricing,
            held.now,
        );
        let report = held.fixtures.settle_alone(
            &held.leases,
            lease,
            &held.runner,
            Cumulative::default(),
            true,
            held.now,
        );
        let (renewed, settled) = tokio::join!(renew, report);
        let renewed = renewed.expect("the renewal is a verdict");

        assert!(
            matches!(settled, Settled::Claimed(_)),
            "the report wins its own lease whatever order: {settled:?}"
        );
        let lines = line_numbers(&held.fixtures, lease).await;
        let dense = lines
            .iter()
            .zip(0_i64..)
            .all(|(seq, expected)| *seq == expected);
        assert!(
            dense && !lines.is_empty(),
            "line numbers stay distinct and dense from zero: {lines:?}, renewed {renewed:?}"
        );
        if matches!(
            renewed,
            Renewed::Extended {
                settlement: Some(_),
                ..
            }
        ) {
            assert!(
                lines.len() <= 2,
                "a renewal and a report log at most one line each: {lines:?}"
            );
        }

        queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
        held.fixtures.cleanup().await;
    }
}
