//! What the ladder asks the datastores at each rung, and at the top.
//!
//! Split from the lane at the file cap: the lane decides WHEN to measure — the
//! rungs, the climb, the observe-only path — and this module decides HOW each
//! reading is taken. A reader following the ladder does not need the shape
//! of an `INFO memory` reply in front of them, and a reader checking the
//! parser does not need the ladder.

use core::time::Duration;
use std::time::Instant;

use afd_fleet::lease::sql::lease::SELECT_READY_CANDIDATES;
use afd_redis::{ReadyIndex, Redis, fleet_stream_key};
use sqlx::Row as _;

use super::count;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::report::Report;

/// Measurement key: `core.fleets` on disk at population, bytes.
pub(super) const FLEETS_TABLE_BYTES: &str = "postgres_fleets_table_bytes";

/// Measurement key: the candidate query's execution time at population.
pub(super) const CANDIDATE_QUERY_MS: &str = "candidate_query_ms";

/// Measurement key: the candidate query's planning time at population.
pub(super) const CANDIDATE_PLAN_MS: &str = "candidate_plan_ms";

/// How many candidates a peek asks for, matching the lease path's own ceiling.
pub(super) const PEEK_COUNT: usize = 64;

/// Samples per latency reading, so a rung reports a median and not one call.
pub(super) const SAMPLES: usize = 20;

/// How the candidate query is explained: with timing, as text, so the two
/// times are read off their own lines. Text rather than JSON because decoding
/// a `json` column needs a sqlx feature the workspace does not enable, and
/// two labelled lines are not worth turning it on for.
pub(super) const EXPLAIN: &str = "EXPLAIN (ANALYZE, FORMAT TEXT) ";

/// The plan line carrying execution time, milliseconds.
pub(super) const EXECUTION_TIME: &str = "Execution Time: ";

/// The plan line carrying planning time, milliseconds.
pub(super) const PLANNING_TIME: &str = "Planning Time: ";

/// The status the candidate query filters on, as the lease path binds it.
pub(super) const ACTIVE: &str = "active";

/// Redis's `used_memory`, in bytes.
pub(super) async fn used_memory(queue: &Redis) -> Result<u64> {
    let mut command = redis::cmd(INFO);
    command.arg(MEMORY);
    let raw: String = queue.command(INFO, MEMORY, &command).await?;
    Ok(raw
        .lines()
        .find_map(|line| line.strip_prefix(USED_MEMORY_FIELD))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0))
}

/// Median readiness-peek latency over [`SAMPLES`] calls.
pub(super) async fn peek_ms(queue: &Redis) -> Result<f64> {
    let index = ReadyIndex::new(queue.clone());
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        index.peek(PEEK_COUNT).await?;
        samples.push(started.elapsed());
    }
    Ok(median_ms(samples))
}

/// Median single-stream read latency over [`SAMPLES`] calls.
pub(super) async fn stream_read_ms(queue: &Redis, fleet: &str) -> Result<f64> {
    let key = fleet_stream_key(fleet);
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let mut command = redis::cmd(XRANGE);
        command
            .arg(&key)
            .arg(RANGE_START)
            .arg(RANGE_END)
            .arg(COUNT)
            .arg(1);
        let started = Instant::now();
        let _entries: Vec<(String, Vec<String>)> = queue.command(XRANGE, &key, &command).await?;
        samples.push(started.elapsed());
    }
    Ok(median_ms(samples))
}

/// Table sizes and the candidate query's plan at population.
pub(super) async fn postgres_at_population(
    stores: &Datastores,
    runner: &str,
    report: &mut Report,
) -> Result<()> {
    report.measurement(
        FLEETS_TABLE_BYTES,
        count(table_sizes(&stores.database).await?),
    );
    let mut connection = stores.database.acquire().await?;
    let ready: Vec<String> = ReadyIndex::new(stores.queue.clone())
        .peek(PEEK_COUNT)
        .await?
        .into_iter()
        .map(|entry| entry.fleet_id)
        .collect();
    // `AssertSqlSafe` because the statement is assembled from two constants
    // this crate and `afd_fleet` own; nothing a caller typed reaches it.
    let lines: Vec<String> = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{EXPLAIN}{SELECT_READY_CANDIDATES}"
    )))
    .bind(ACTIVE)
    .bind(runner)
    .bind(&ready)
    .bind(i64::try_from(PEEK_COUNT).unwrap_or(i64::MAX))
    .fetch_all(&mut *connection)
    .await?
    .iter()
    .filter_map(|row| row.try_get::<String, _>(0).ok())
    .collect();
    if let Some(execution) = plan_time(&lines, EXECUTION_TIME) {
        report.measurement(CANDIDATE_QUERY_MS, execution);
    }
    if let Some(planning) = plan_time(&lines, PLANNING_TIME) {
        report.measurement(CANDIDATE_PLAN_MS, planning);
    }
    Ok(())
}

/// The milliseconds a labelled plan line reports, if the plan carried one.
///
/// Absent rather than zero when the line is missing: a plan without timing is
/// a plan nothing timed, and RULE ECL says that is not a measurement.
pub(super) fn plan_time(lines: &[String], label: &str) -> Option<f64> {
    lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(label))
        .and_then(|rest| rest.trim_end_matches(" ms").parse().ok())
}

/// `core.fleets` with its indexes, in bytes.
pub(super) async fn table_sizes(database: &afd_db::Db) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let bytes: i64 = sqlx::query(TABLE_SIZE_QUERY)
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?;
    Ok(u64::try_from(bytes).unwrap_or(0))
}

/// The middle sample, in milliseconds.
pub(super) fn median_ms(mut samples: Vec<Duration>) -> f64 {
    samples.sort();
    samples
        .get(samples.len() / 2)
        .map_or(0.0, |sample| sample.as_secs_f64() * MILLIS_PER_SECOND)
}

/// Milliseconds in a second, for reporting a sampled duration.
pub(super) const MILLIS_PER_SECOND: f64 = 1_000.0;

/// `INFO`, for the memory section.
pub(super) const INFO: &str = "INFO";

/// The `INFO` section reporting memory.
pub(super) const MEMORY: &str = "memory";

/// The line carrying resident bytes.
pub(super) const USED_MEMORY_FIELD: &str = "used_memory:";

/// Read a stream by range.
pub(super) const XRANGE: &str = "XRANGE";

/// The smallest stream id.
pub(super) const RANGE_START: &str = "-";

/// The largest stream id.
pub(super) const RANGE_END: &str = "+";

/// Cap a range read.
pub(super) const COUNT: &str = "COUNT";

/// `core.fleets` and every index on it, in bytes.
pub(super) const TABLE_SIZE_QUERY: &str = "SELECT pg_total_relation_size('core.fleets')";
