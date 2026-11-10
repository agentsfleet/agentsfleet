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

use af_core::id::Uuid7;
use afd_fleet::lease::sql::meter::Metering;
use afd_fleet::lease::{Renewed, Settled};

use crate::report_seed::held;

/// How many times each race runs, so one lucky interleaving cannot pass it.
const ROUNDS: usize = 5;

/// The usage lines `lease` has logged, by line number.
/// The lease's lines as `(kind, seq)`, model lines first.
async fn line_numbers(fixtures: &crate::support::Fixtures, lease: &Uuid7) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT kind, seq FROM billing.usage_lines WHERE work_id = $1 ORDER BY kind, seq",
    )
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

const MODEL: &str = af_wire::usage::LineKind::Model.as_str();
const RUNTIME: &str = af_wire::usage::LineKind::Runtime.as_str();

const REQUEST_INPUT_TOKENS: u32 = 1_000;

/// One provider request, numbered `seq`, as a runner reports it.
const fn request(held: &crate::report_seed::Held, seq: u32) -> af_wire::usage::RequestUsage {
    af_wire::usage::RequestUsage {
        seq,
        at: held.now,
        input_tokens: REQUEST_INPUT_TOKENS,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: 100,
    }
}

/// A runner re-sends its report, carrying the same request, while the first
/// is still in flight: exactly one claims the lease and logs its lines, the
/// other is told the lease is already settled, and the lease holds the request
/// once and one runtime line, not two of either.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_reports_of_one_lease_log_one_final_line() {
    for _round in 0..ROUNDS {
        let held = held().await;
        held.run_a_slice().await;
        let lease = &held.issued.lease_id;
        let requests = [request(&held, 0)];
        let report = || {
            held.fixtures
                .settle_alone(&held.leases, lease, &held.runner, &requests, true, held.now)
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
        let lines = line_numbers(&held.fixtures, lease).await;
        let kinds: Vec<&str> = lines.iter().map(|(kind, _seq)| kind.as_str()).collect();
        assert_eq!(
            kinds,
            [MODEL, RUNTIME],
            "one request and one slice: {lines:?}"
        );
        assert_eq!(lines.first().map(|line| line.1), Some(0), "{lines:?}");

        held.fixtures.cleanup().await;
    }
}

/// A renewal and the report race on one lease, both carrying the same request:
/// the request is stored once, and each pass numbers its runtime line by the
/// slice it bills, so the lease never holds two lines under one number and
/// never loses one to the `(work, kind, seq)` guard. The second pass
/// may measure no runtime, and log no runtime line, when the two land within a
/// millisecond.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_racing_the_report_numbers_its_line_apart() {
    for _round in 0..ROUNDS {
        let held = held().await;
        held.run_a_slice().await;
        let lease = &held.issued.lease_id;
        let requests = [request(&held, 0)];
        let renew = held.leases.extend(
            Metering {
                lease_id: lease,
                runner_id: &held.runner,
                requests: &requests,
            },
            &held.lease_key,
            &held.pricing,
            held.now,
        );
        let report = held.fixtures.settle_alone(
            &held.leases,
            lease,
            &held.runner,
            &requests,
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
        let seqs_of = |wanted: &str| -> Vec<i64> {
            lines
                .iter()
                .filter(|(kind, _seq)| kind == wanted)
                .map(|line| line.1)
                .collect()
        };
        assert_eq!(
            seqs_of(MODEL),
            [0],
            "the shared request is stored once: {lines:?}"
        );
        let runtime = seqs_of(RUNTIME);
        assert!(
            !runtime.is_empty() && runtime.windows(2).all(|pair| pair.first() < pair.last()),
            "each slice is its own runtime line, by its start: {lines:?}, renewed {renewed:?}"
        );
        if matches!(
            renewed,
            Renewed::Extended {
                settlement: Some(_),
                ..
            }
        ) {
            assert!(
                runtime.len() <= 2,
                "a renewal and a report log at most one runtime line each: {lines:?}"
            );
        }

        held.fixtures.cleanup().await;
    }
}
