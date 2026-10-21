//! What the ladder asks the datastores at each rung, and at the top.
//!
//! Split from the lane at the file cap: the lane decides WHEN to measure — the
//! rungs, the climb, the observe-only path — and this module decides HOW each
//! reading is taken.

use core::time::Duration;
use std::time::Instant;

use afd_fleet::lease::assign::MAX_CANDIDATES_PER_POLL;
use afd_fleet::lease::sql::queue::SELECT_CANDIDATES;

use crate::datastores::{Datastores, postgres_count};
use crate::error::{ErrorKind, Result};
use crate::report::Report;

/// Measurement key: `fleet.fleets` on disk at population, bytes.
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

/// Milliseconds in a second, for reporting a sampled duration.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// `fleet.fleets` and every index on it, in bytes.
const TABLE_SIZE_QUERY: &str = include_str!("sql/select_fleets_size.sql");

/// How many fleets exist, whether they have queued work.
const POPULATION_QUERY: &str = include_str!("sql/select_fleet_population.sql");

/// The store named when a latency sample set is empty.
const POSTGRES: &str = "postgres";
const SELECT_ADMISSION_HEAD: &str = include_str!("sql/select_admission_head.sql");

/// Median candidate discovery latency over the serving query.
pub(super) async fn candidate_ms(database: &afd_db::Db, runner: &str) -> Result<f64> {
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut connection = database.acquire().await?;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let _candidates: Vec<String> = sqlx::query_scalar(SELECT_CANDIDATES)
            .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
            .bind(runner)
            .bind(i64::try_from(MAX_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
            .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
            .bind(Vec::<String>::new())
            .bind(None::<&str>)
            .fetch_all(&mut *connection)
            .await?;
        samples.push(started.elapsed());
    }
    median_ms(samples).ok_or_else(|| {
        ErrorKind::CounterUnreadable {
            datastore: POSTGRES,
            field: "candidate samples",
        }
        .into()
    })
}

/// Median indexed head lookup latency over the unsettled ledger.
pub(super) async fn admission_read_ms(database: &afd_db::Db, fleet: &str) -> Result<f64> {
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut connection = database.acquire().await?;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let _head: Option<String> = sqlx::query_scalar(SELECT_ADMISSION_HEAD)
            .bind(fleet)
            .fetch_optional(&mut *connection)
            .await?;
        samples.push(started.elapsed());
    }
    median_ms(samples).ok_or_else(|| {
        ErrorKind::CounterUnreadable {
            datastore: POSTGRES,
            field: "admission head samples",
        }
        .into()
    })
}

/// Table sizes and the candidate query's plan at population.
pub(super) async fn postgres_at_population(
    stores: &Datastores,
    runner: &str,
    report: &mut Report,
) -> Result<()> {
    report.measurement(
        FLEETS_TABLE_BYTES,
        crate::report::count(table_sizes(&stores.database).await?),
    );
    let mut connection = stores.database.acquire().await?;
    // `AssertSqlSafe` because the statement is assembled from two constants
    // this crate and `afd_fleet` own; nothing a caller typed reaches it.
    let lines: Vec<String> =
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("{EXPLAIN}{SELECT_CANDIDATES}")))
            .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
            .bind(runner)
            .bind(i64::try_from(MAX_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            // How long a silent holder binds, and the status a holder must be in to
            // bind at all: the lease path's own binds for the hold condition. The
            // instant they compare against is the statement's own clock.
            .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
            .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
            .bind(Vec::<String>::new())
            .bind(None::<&str>)
            .fetch_all(&mut *connection)
            .await?;
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
pub(crate) fn plan_time(lines: &[String], label: &str) -> Option<f64> {
    lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(label))
        .and_then(|rest| rest.trim_end_matches(" ms").parse().ok())
}

/// `fleet.fleets` with its indexes, in bytes.
pub(super) async fn table_sizes(database: &afd_db::Db) -> Result<u64> {
    postgres_count(
        scalar(database, TABLE_SIZE_QUERY).await?,
        "pg_total_relation_size",
    )
}

/// How many fleets the database holds.
pub(super) async fn fleet_population(database: &afd_db::Db) -> Result<u64> {
    postgres_count(scalar(database, POPULATION_QUERY).await?, "count(*)")
}

/// One bigint out of a constant statement.
async fn scalar(database: &afd_db::Db, statement: &'static str) -> Result<i64> {
    let mut connection = database.acquire().await?;
    Ok(sqlx::query_scalar(statement)
        .fetch_one(&mut *connection)
        .await?)
}

/// The middle sample in milliseconds, or nothing for no samples.
pub(crate) fn median_ms(mut samples: Vec<Duration>) -> Option<f64> {
    samples.sort();
    samples
        .get(samples.len() / 2)
        .map(|sample| sample.as_secs_f64() * MILLIS_PER_SECOND)
}
