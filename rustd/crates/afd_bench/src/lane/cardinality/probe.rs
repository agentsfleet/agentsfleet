//! What the ladder asks the datastores at each rung, and at the top.
//!
//! Split from the lane at the file cap: the lane decides WHEN to measure — the
//! rungs, the climb, the observe-only path — and this module decides HOW each
//! reading is taken.

use core::time::Duration;
use std::time::Instant;

use afd_fleet::lease::assign::MAX_READY_CANDIDATES_PER_POLL;
use afd_fleet::lease::sql::lease::SELECT_READY_CANDIDATES;
use afd_redis::{ReadyIndex, Redis, fleet_stream_key};
use sqlx::Row as _;

use crate::datastores::Datastores;
use crate::datastores::command::{COUNT, RANGE_END, RANGE_START, XRANGE};
use crate::error::{Error, Result};
use crate::report::Report;

/// Measurement key: `core.fleets` on disk at population, bytes.
pub(super) const FLEETS_TABLE_BYTES: &str = "postgres_fleets_table_bytes";

/// Measurement key: the candidate query's execution time at population.
const CANDIDATE_QUERY_MS: &str = "candidate_query_ms";

/// Measurement key: the candidate query's planning time at population.
const CANDIDATE_PLAN_MS: &str = "candidate_plan_ms";

/// Samples per latency reading, so a rung reports a median and not one call.
const SAMPLES: usize = 20;

/// How the candidate query is explained: with timing, as text, so the two
/// times are read off their own lines. Text rather than JSON because decoding
/// a `json` column needs a sqlx feature the workspace does not enable, and
/// two labelled lines are not worth turning it on for.
const EXPLAIN: &str = "EXPLAIN (ANALYZE, FORMAT TEXT) ";

/// The plan line carrying execution time, milliseconds.
pub(super) const EXECUTION_TIME: &str = "Execution Time: ";

/// The plan line carrying planning time, milliseconds.
pub(super) const PLANNING_TIME: &str = "Planning Time: ";

/// The status the candidate query filters on, as the lease path binds it.
const ACTIVE: &str = "active";

/// Milliseconds in a second, for reporting a sampled duration.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// `core.fleets` and every index on it, in bytes.
const TABLE_SIZE_QUERY: &str = "SELECT pg_total_relation_size('core.fleets')";

/// How many fleets exist, whatever their readiness.
const POPULATION_QUERY: &str = "SELECT count(*) FROM core.fleets";

/// The datastore named when a Postgres reading will not parse.
const POSTGRES: &str = "postgres";

/// Median readiness-peek latency over [`SAMPLES`] calls, asking for the same
/// number of candidates the lease path asks for.
pub(super) async fn peek_samples(queue: &Redis) -> Result<Vec<Duration>> {
    let index = ReadyIndex::new(queue.clone());
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        index.peek(MAX_READY_CANDIDATES_PER_POLL).await?;
        samples.push(started.elapsed());
    }
    Ok(samples)
}

/// Median single-stream read latency over [`SAMPLES`] calls.
pub(super) async fn stream_read_samples(queue: &Redis, fleet: &str) -> Result<Vec<Duration>> {
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
    Ok(samples)
}

/// Table sizes and the candidate query's plan at population.
pub(super) async fn postgres_at_population(
    stores: &Datastores,
    runner: &str,
    report: &mut Report,
) -> Result<()> {
    report.count(FLEETS_TABLE_BYTES, table_sizes(&stores.database).await?);
    let mut connection = stores.database.acquire().await?;
    let ready: Vec<String> = ReadyIndex::new(stores.queue.clone())
        .peek(MAX_READY_CANDIDATES_PER_POLL)
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
    .bind(i64::try_from(MAX_READY_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
    .fetch_all(&mut *connection)
    .await?
    .iter()
    .filter_map(|row| row.try_get::<String, _>(0).ok())
    .collect();
    if let Some(execution) = plan_time(&lines, EXECUTION_TIME) {
        report.calculated(
            CANDIDATE_QUERY_MS,
            execution,
            crate::report::Calculation::ParsedMillis {
                label: EXECUTION_TIME.to_owned(),
                lines: lines.clone(),
            },
        );
    }
    if let Some(planning) = plan_time(&lines, PLANNING_TIME) {
        report.calculated(
            CANDIDATE_PLAN_MS,
            planning,
            crate::report::Calculation::ParsedMillis {
                label: PLANNING_TIME.to_owned(),
                lines,
            },
        );
    }
    Ok(())
}

/// The milliseconds a labelled plan line reports, if the plan carried one.
///
/// Absent rather than zero when the line is missing: a plan without timing is
/// a plan nothing timed, and RULE ECL says that is not a measurement.
pub(crate) fn plan_time(lines: &[String], label: &str) -> Option<f64> {
    lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(label))
        .and_then(|rest| rest.trim_end_matches(" ms").parse().ok())
}

/// `core.fleets` with its indexes, in bytes.
pub(super) async fn table_sizes(database: &afd_db::Db) -> Result<u64> {
    non_negative(
        scalar(database, TABLE_SIZE_QUERY).await?,
        "pg_total_relation_size",
    )
}

/// How many fleets the database holds.
pub(super) async fn fleet_population(database: &afd_db::Db) -> Result<u64> {
    non_negative(scalar(database, POPULATION_QUERY).await?, "count(*)")
}

/// One bigint out of a constant statement.
async fn scalar(database: &afd_db::Db, statement: &'static str) -> Result<i64> {
    let mut connection = database.acquire().await?;
    Ok(sqlx::query(statement)
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?)
}

/// A Postgres count that cannot honestly be negative.
fn non_negative(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_negative| Error::CounterUnreadable {
        datastore: POSTGRES,
        field,
    })
}

/// The middle sample in milliseconds, or nothing for no samples.
pub(crate) fn median_ms(mut samples: Vec<Duration>) -> Option<f64> {
    samples.sort();
    samples
        .get(samples.len() / 2)
        .map(|sample| sample.as_secs_f64() * MILLIS_PER_SECOND)
}
