//! What an idle fleet costs when there are a great many of them.
//!
//! # A ladder, not a jump
//!
//! The population is created in rungs, and every rung reports the same three
//! things: Redis memory per fleet, the readiness peek's latency, and one
//! stream's read latency. A single number at a million would say what a
//! million costs; the ladder says whether the cost is LINEAR, which is the
//! question the per-fleet stream and consumer group design actually hangs on.
//!
//! # The top rung reads Postgres
//!
//! Table sizes, and the candidate query's plan and execution time at
//! population — the real `SELECT_READY_CANDIDATES`, bound as the lease path
//! binds it, under `EXPLAIN ANALYZE`. It is measured once, at the top, because
//! the rungs below are subsets of the same rows and the plan does not change.
//!
//! # A deployed profile observes and creates nothing
//!
//! Creating a million streams in a shared environment is not a measurement
//! anyone consented to. Against a deployed target the lane reads what is
//! there, reports it in the same shape, and says `created: false`.

use core::time::Duration;
use std::time::Instant;

use afd_fleet::lease::sql::lease::SELECT_READY_CANDIDATES;
use afd_redis::{ReadyIndex, Redis, fleet_stream_key};
use sqlx::Row as _;

use crate::datastores::Datastores;
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::seed;
use crate::profile::{Parameter, Profile, Target};
use crate::report::{Fixture, Lane, Report};

/// Series key: the population at each rung.
const LADDER: &str = "ladder_fleets";

/// Series key: Redis bytes per fleet at each rung, over the rung below.
const BYTES_PER_FLEET: &str = "redis_bytes_per_fleet";

/// Series key: readiness peek latency at each rung, milliseconds.
const PEEK_MS: &str = "peek_ms";

/// Series key: one stream's read latency at each rung, milliseconds.
const STREAM_READ_MS: &str = "stream_read_ms";

/// Measurement key: Redis bytes the whole population added.
const REDIS_BYTES_TOTAL: &str = "redis_bytes_total";

/// Measurement key: `core.fleets` on disk at population, bytes.
const FLEETS_TABLE_BYTES: &str = "postgres_fleets_table_bytes";

/// Measurement key: the candidate query's execution time at population.
const CANDIDATE_QUERY_MS: &str = "candidate_query_ms";

/// Measurement key: the candidate query's planning time at population.
const CANDIDATE_PLAN_MS: &str = "candidate_plan_ms";

/// How many rungs the ladder has below its ceiling, each ten times the last.
///
/// Three, so a ceiling of a million is reached through 1 000, 10 000 and
/// 100 000 — enough points to see a slope, few enough that seeding stays a
/// fraction of the run.
const RUNGS_BELOW_CEILING: u32 = 3;

/// How many candidates a peek asks for, matching the lease path's own ceiling.
const PEEK_COUNT: usize = 64;

/// Samples per latency reading, so a rung reports a median and not one call.
const SAMPLES: usize = 20;

/// The clock a seeded row is stamped with.
const SEEDED_AT: i64 = 1_767_225_600_000;

/// How the candidate query is explained: with timing, as text, so the two
/// times are read off their own lines. Text rather than JSON because decoding
/// a `json` column needs a sqlx feature the workspace does not enable, and
/// two labelled lines are not worth turning it on for.
const EXPLAIN: &str = "EXPLAIN (ANALYZE, FORMAT TEXT) ";

/// The plan line carrying execution time, milliseconds.
const EXECUTION_TIME: &str = "Execution Time: ";

/// The plan line carrying planning time, milliseconds.
const PLANNING_TIME: &str = "Planning Time: ";

/// The status the candidate query filters on, as the lease path binds it.
const ACTIVE: &str = "active";

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// The ladder's top rung.
    pub fleets: u64,
}

impl Parameters {
    /// Refuse a ceiling above the profile's cap, before a connection opens.
    ///
    /// # Errors
    ///
    /// [`crate::Error::CapExceeded`] naming the cap and the profile.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Fleets, self.fleets)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, or a datastore that would not answer.
pub async fn run(
    profile: Profile,
    target: &Target,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    let mut report = Report::new(Lane::Cardinality, profile);
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    let mut ledger = FixtureLedger::new();

    match target {
        Target::Rig => {
            report.created = true;
            climb(stores, prefix, parameters, &mut report, &mut ledger).await?;
        }
        Target::Deployed { .. } => {
            report.created = false;
            observe(stores, &mut report).await?;
        }
    }
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// The rungs up to and including the ceiling.
fn rungs(ceiling: u64) -> Vec<u64> {
    let mut rungs: Vec<u64> = (1..=RUNGS_BELOW_CEILING)
        .rev()
        .map(|below| ceiling / 10_u64.pow(below))
        .filter(|rung| *rung > 0)
        .collect();
    rungs.push(ceiling);
    rungs.dedup();
    rungs
}

/// Seed rung by rung, measuring at each.
async fn climb(
    stores: &Datastores,
    prefix: &RunPrefix,
    parameters: Parameters,
    report: &mut Report,
    ledger: &mut FixtureLedger,
) -> Result<()> {
    let tag = seed::placement_tag(prefix);
    let runner = seed::runner(&stores.database, &prefix.name("host"), &tag, SEEDED_AT).await?;
    ledger.created(seed::ROWS_PER_RUNNER);
    let baseline = used_memory(&stores.queue).await?;

    let mut seeded_to = 0;
    let mut previous_bytes = baseline;
    let mut previous_rung = 0;
    let mut last_fleet = String::new();
    for rung in rungs(parameters.fleets) {
        for index in seeded_to..rung {
            let fleet = seed::ready_fleet(
                &stores.database,
                &stores.queue,
                prefix,
                &tag,
                index,
                SEEDED_AT,
            )
            .await?;
            last_fleet = fleet.fleet;
            ledger.created(seed::ROWS_PER_FLEET);
        }
        seeded_to = rung;

        let bytes = used_memory(&stores.queue).await?;
        let added = bytes.saturating_sub(previous_bytes);
        let fleets_added = rung.saturating_sub(previous_rung);
        push(report, LADDER, count(rung));
        push(report, BYTES_PER_FLEET, ratio(added, fleets_added));
        push(report, PEEK_MS, peek_ms(&stores.queue).await?);
        push(
            report,
            STREAM_READ_MS,
            stream_read_ms(&stores.queue, &last_fleet).await?,
        );
        previous_bytes = bytes;
        previous_rung = rung;
    }
    report.measurement(
        REDIS_BYTES_TOTAL,
        count(previous_bytes.saturating_sub(baseline)),
    );
    postgres_at_population(stores, &runner.to_string(), report).await
}

/// Read the population that is already there, creating nothing.
async fn observe(stores: &Datastores, report: &mut Report) -> Result<()> {
    let population = ReadyIndex::new(stores.queue.clone()).len().await?;
    push(report, LADDER, count(population));
    push(report, PEEK_MS, peek_ms(&stores.queue).await?);
    let sizes = table_sizes(&stores.database).await?;
    report.measurement(FLEETS_TABLE_BYTES, count(sizes));
    Ok(())
}

/// Append one sample to a series.
fn push(report: &mut Report, series: &str, value: f64) {
    report
        .series
        .entry(series.to_owned())
        .or_default()
        .push(value);
}

/// Redis's `used_memory`, in bytes.
async fn used_memory(queue: &Redis) -> Result<u64> {
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
async fn peek_ms(queue: &Redis) -> Result<f64> {
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
async fn stream_read_ms(queue: &Redis, fleet: &str) -> Result<f64> {
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
async fn postgres_at_population(
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
fn plan_time(lines: &[String], label: &str) -> Option<f64> {
    lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(label))
        .and_then(|rest| rest.trim_end_matches(" ms").parse().ok())
}

/// `core.fleets` with its indexes, in bytes.
async fn table_sizes(database: &afd_db::Db) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let bytes: i64 = sqlx::query(TABLE_SIZE_QUERY)
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?;
    Ok(u64::try_from(bytes).unwrap_or(0))
}

/// The middle sample, in milliseconds.
fn median_ms(mut samples: Vec<Duration>) -> f64 {
    samples.sort();
    samples
        .get(samples.len() / 2)
        .map_or(0.0, |sample| sample.as_secs_f64() * MILLIS_PER_SECOND)
}

/// A count as a ratio's operand.
fn count(value: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count past f64's exact range is not a population this ladder reaches"
    )]
    {
        value as f64
    }
}

/// `numerator / denominator`, or zero when nothing was added.
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    count(numerator) / count(denominator)
}

/// Milliseconds in a second, for reporting a sampled duration.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// `INFO`, for the memory section.
const INFO: &str = "INFO";

/// The `INFO` section reporting memory.
const MEMORY: &str = "memory";

/// The line carrying resident bytes.
const USED_MEMORY_FIELD: &str = "used_memory:";

/// Read a stream by range.
const XRANGE: &str = "XRANGE";

/// The smallest stream id.
const RANGE_START: &str = "-";

/// The largest stream id.
const RANGE_END: &str = "+";

/// Cap a range read.
const COUNT: &str = "COUNT";

/// `core.fleets` and every index on it, in bytes.
const TABLE_SIZE_QUERY: &str = "SELECT pg_total_relation_size('core.fleets')";
