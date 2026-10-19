//! A run's usage lines across several renewals and its report, billed once.
//!
//! A run that ends with more lines than one report carries renews the oldest
//! away first, a body at a time, then reports the rest. A renewal whose
//! answer was lost is sent again, and the report may repeat a line a renewal
//! already took. Every line must still be logged, charged and debited once.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::report::RunTokens;
use afd_wire::report::{RenewRequest, ReportRequest};
use afd_wire::usage::{DAEMON_SEQ_BASE, LINES_MAX, RequestLines, RequestUsage};

use crate::integration_held_sandbox::report::processed;
use crate::queue;
use crate::report_seed::{DEEP_POOL, Held, held};
use crate::seed::{MODEL, PROVIDER};

/// Lines past two full bodies: two renewals and a report.
const LINES: usize = 2 * LINES_MAX + 76;

/// Where the report's lines start: inside the second renewal's body, so it
/// repeats lines that renewal already took.
const REPORT_FROM: usize = 2 * LINES_MAX - 24;

/// A daily ceiling no run here reaches, so the renewals are about the lines.
const DEEP_DAILY_BUDGET: &str = r#"{"name":"usage-flush","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#;

/// Each request's tokens.
const INPUT_TOKENS: u32 = 1_000;
const OUTPUT_TOKENS: u32 = 100;

/// Per request line logged for this lease: how many rows, how many settled,
/// and how many distinct charges.
const REQUEST_LINES: &str = "SELECT count(*), count(*) FILTER (WHERE settled_at IS NOT NULL), \
     count(DISTINCT charged_nanos), COALESCE(min(seq), -1), COALESCE(max(seq), -1), \
     COALESCE(sum(input_token_count), 0)::BIGINT, COALESCE(min(charged_nanos), 0) \
     FROM billing.usage_lines WHERE work_id = $1::uuid AND seq < $2";

/// Every nano this lease's lines charged, runtime lines included.
const CHARGED: &str = "SELECT COALESCE(sum(charged_nanos), 0)::BIGINT \
     FROM billing.usage_lines WHERE work_id = $1::uuid";

/// Requests `from..to`, as a runner numbers them.
fn requests(held: &Held, from: usize, to: usize) -> RequestLines {
    RequestLines(
        (from..to)
            .map(|seq| RequestUsage {
                seq: u32::try_from(seq).expect("a test line number fits the wire"),
                at: held.now,
                input_tokens: INPUT_TOKENS,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
                output_tokens: OUTPUT_TOKENS,
            })
            .collect(),
    )
}

/// Renews `held`'s lease carrying requests `from..to`.
async fn renew(held: &Held, plane: &afd_fleet::lease::Plane, from: usize, to: usize) {
    plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            RenewRequest {
                requests: requests(held, from, to),
            },
            held.now,
        )
        .await
        .expect("a funded lease renews");
}

/// Two full renewals, the first sent twice, then a report that repeats part
/// of the second: every one of the run's lines is logged, settled and charged
/// once, and the wallet is debited exactly what the lines charged.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lines_across_renewals_and_the_report_are_billed_once_each() {
    let held = held().await;
    held.fixtures
        .native_config(&held.fleet, DEEP_DAILY_BUDGET)
        .await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let plane = held.fixtures.plane();
    let lease = held.issued.lease_id.as_str();

    renew(&held, &plane, 0, LINES_MAX).await;
    // The answer was lost, so the runner sends the same body again.
    renew(&held, &plane, 0, LINES_MAX).await;
    renew(&held, &plane, LINES_MAX, 2 * LINES_MAX).await;
    let report = ReportRequest {
        requests: requests(&held, REPORT_FROM, LINES),
        ..processed(&held, None)
    };
    plane
        .report(&held.runner, &report, held.now)
        .await
        .expect("the report settles the lease");

    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let (rows, settled, distinct_prices, first, last, input, per_line): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(REQUEST_LINES)
        .bind(lease)
        .bind(i64::from(DAEMON_SEQ_BASE))
        .fetch_one(&mut *connection)
        .await
        .expect("the lines read");
    let lines = i64::try_from(LINES).expect("a test line count fits");
    assert_eq!(rows, lines, "one row per request, none twice");
    assert_eq!((first, last), (0, lines - 1), "none missing");
    assert_eq!(input, lines * i64::from(INPUT_TOKENS));
    assert_eq!(settled, rows, "every line is settled");
    assert_eq!(distinct_prices, 1, "identical requests charge alike");
    assert!(per_line > 0, "the seeded rate prices a request");

    let total: i64 = sqlx::query_scalar(CHARGED)
        .bind(lease)
        .fetch_one(&mut *connection)
        .await
        .expect("the charge reads");
    drop(connection);
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - total),
        "the wallet pays each line once"
    );

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A request that read and wrote the provider's cache in every class.
const CACHED_REQUEST: RequestUsage = RequestUsage {
    seq: 0,
    at: jiff::Timestamp::UNIX_EPOCH,
    input_tokens: 700,
    cached_input_tokens: 300,
    cache_write_tokens: 40,
    cache_write_5m_tokens: 50,
    cache_write_1h_tokens: 60,
    output_tokens: 20,
};

/// A report whose lines wrote the cache answers those writes in the run's
/// tokens, every lifetime summed, so its metrics count them as input.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_whose_lines_write_the_cache_counts_the_writes() {
    let held = held().await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let plane = held.fixtures.plane();
    let line = |seq| RequestUsage {
        seq,
        at: held.now,
        ..CACHED_REQUEST
    };
    let report = ReportRequest {
        requests: RequestLines(vec![line(0), line(1)]),
        ..processed(&held, None)
    };

    let settled = plane
        .report(&held.runner, &report, held.now)
        .await
        .expect("the report settles the lease");

    assert_eq!(
        settled.tokens,
        RunTokens {
            input_tokens: 1_400,
            cached_input_tokens: 600,
            cache_write_tokens: 300,
            output_tokens: 40,
        },
        "each class summed over both lines, writes across their lifetimes"
    );
    assert_eq!(settled.tokens.prompt(), 2_300, "reads and writes are input");

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
