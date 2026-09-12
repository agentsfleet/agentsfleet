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
//! there — the fleet population from Postgres, the readiness depth from Redis,
//! each under its own name — reports it in the same shape, and says
//! `created: false`.

mod probe;

use afd_redis::ReadyIndex;
use tokio_util::sync::CancellationToken;

use self::probe::{
    FLEETS_TABLE_BYTES, median_ms, peek_samples, postgres_at_population, stream_read_samples,
    table_sizes,
};
use crate::datastores::{Datastores, redis_used_memory};
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::seed::{self, ROWS_PER_FLEET, ROWS_PER_RUNNER, SEEDED_AT};
use crate::profile::{Parameter, Profile, Target};
use crate::report::{Calculation, Fixture, Lane, Report, count};

/// Series key: the fleet population at each rung.
const LADDER: &str = "ladder_fleets";

/// Series key: Redis bytes per fleet at each rung, over the rung below.
const BYTES_PER_FLEET: &str = "redis_bytes_per_fleet";

/// Series key: readiness peek latency at each rung, milliseconds.
const PEEK_MS: &str = "peek_ms";

/// Series key: one stream's read latency at each rung, milliseconds.
const STREAM_READ_MS: &str = "stream_read_ms";

/// Measurement key: Redis bytes the whole population added.
const REDIS_BYTES_TOTAL: &str = "redis_bytes_total";

/// Measurement key: how many fleets the readiness index holds on a deployed
/// target. Its own name, because it is not the population.
const READY_DEPTH: &str = "ready_depth";
const REDIS: &str = "redis";
const PEEK_SAMPLE_FIELD: &str = "readiness peek samples";

/// How many rungs the ladder has below its ceiling, each ten times the last.
///
/// Three, so a ceiling of a million is reached through 1 000, 10 000 and
/// 100 000 — enough points to see a slope, few enough that seeding stays a
/// fraction of the run.
const RUNGS_BELOW_CEILING: u32 = 3;

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// The ladder's top rung.
    pub fleets: u64,
}

impl Parameters {
    /// Refuse a ceiling outside the profile's bounds, before a connection opens.
    ///
    /// # Errors
    ///
    /// A cap or a floor, named.
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
    run_cancelled(
        profile,
        target,
        parameters,
        stores,
        prefix,
        CancellationToken::new(),
    )
    .await
}

/// Run the lane with an operator cancellation token.
///
/// # Errors
///
/// The same failures as [`run`], plus [`crate::Error::Cancelled`] while the
/// local population is growing.
pub async fn run_cancelled(
    profile: Profile,
    target: &Target,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
    cancellation: CancellationToken,
) -> Result<Report> {
    parameters.admit(profile)?;
    let mut report = Report::new(Lane::Cardinality, profile);
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    let mut ledger = FixtureLedger::new();

    match target {
        Target::Rig => {
            report.created = true;
            climb(
                stores,
                prefix,
                parameters,
                &cancellation,
                &mut report,
                &mut ledger,
            )
            .await?;
        }
        Target::Deployed { .. } => {
            report.created = false;
            observe(stores, &mut report).await?;
        }
    }
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// The rungs up to and including the ceiling; none for a ceiling of zero.
pub(crate) fn rungs(ceiling: u64) -> Vec<u64> {
    if ceiling == 0 {
        return Vec::new();
    }
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
    cancellation: &CancellationToken,
    report: &mut Report,
    ledger: &mut FixtureLedger,
) -> Result<()> {
    let tag = seed::placement_tag(prefix);
    let runner = seed::runner(&stores.database, &prefix.name("host"), &tag, SEEDED_AT).await?;
    ledger.created(ROWS_PER_RUNNER);
    let baseline = redis_used_memory(&stores.queue).await?;

    let mut seeded_to = 0;
    let mut previous_bytes = baseline;
    let mut previous_rung = 0;
    let mut last_fleet = String::new();
    for rung in rungs(parameters.fleets) {
        for index in seeded_to..rung {
            if cancellation.is_cancelled() {
                return Err(crate::Error::Cancelled);
            }
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
            ledger.created(ROWS_PER_FLEET);
        }
        seeded_to = rung;

        let bytes = redis_used_memory(&stores.queue).await?;
        let added = bytes.saturating_sub(previous_bytes);
        let fleets_added = rung.saturating_sub(previous_rung);
        push(report, LADDER, count(rung), Calculation::count(rung));
        push(
            report,
            BYTES_PER_FLEET,
            Calculation::ratio_value(added, fleets_added),
            Calculation::ratio(added, fleets_added),
        );
        let peek = peek_samples(&stores.queue).await?;
        push(
            report,
            PEEK_MS,
            median_ms(&peek).ok_or(crate::Error::CounterUnreadable {
                datastore: REDIS,
                field: PEEK_SAMPLE_FIELD,
            })?,
            Calculation::median(&peek),
        );
        let stream = stream_read_samples(&stores.queue, &last_fleet).await?;
        push(
            report,
            STREAM_READ_MS,
            median_ms(&stream).ok_or(crate::Error::CounterUnreadable {
                datastore: REDIS,
                field: "stream read samples",
            })?,
            Calculation::median(&stream),
        );
        previous_bytes = bytes;
        previous_rung = rung;
    }
    report.difference(REDIS_BYTES_TOTAL, baseline, previous_bytes);
    postgres_at_population(stores, &runner.to_string(), report).await
}

/// Read the population that is already there, creating nothing.
async fn observe(stores: &Datastores, report: &mut Report) -> Result<()> {
    let population = probe::fleet_population(&stores.database).await?;
    push(
        report,
        LADDER,
        count(population),
        Calculation::count(population),
    );
    report.count(
        READY_DEPTH,
        ReadyIndex::new(stores.queue.clone()).len().await?,
    );
    let peek = peek_samples(&stores.queue).await?;
    push(
        report,
        PEEK_MS,
        median_ms(&peek).ok_or(crate::Error::CounterUnreadable {
            datastore: REDIS,
            field: PEEK_SAMPLE_FIELD,
        })?,
        Calculation::median(&peek),
    );
    report.count(FLEETS_TABLE_BYTES, table_sizes(&stores.database).await?);
    Ok(())
}

/// Append one sample to a series.
fn push(report: &mut Report, series: &str, value: f64, calculation: Calculation) {
    report.series_value(series, value, calculation);
}

#[cfg(test)]
mod tests;
