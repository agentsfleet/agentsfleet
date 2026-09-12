//! What steer ingress accepts under concurrency, and where a steer costs.
//!
//! # A steer is two Redis commands and no Postgres
//!
//! `afd_events::Steer::append` issues an `XADD` onto the fleet's stream and an
//! `HSET` marking the fleet ready. That is the whole path — its own module note
//! says "Nothing is written to Postgres here", and `Steer::new` takes only a
//! `Redis` handle. A steer becomes a row when a runner LEASES it, and that cost
//! belongs to the lease lane, which already counts it.
//!
//! So the Postgres number this lane reports is expected to be near zero, and
//! it is read from `pg_stat_database` rather than assumed: a zero nobody
//! measured is indistinguishable from a measurement nobody took.
//!
//! # The readiness index is the interesting part
//!
//! Every steer, for every fleet, writes a field into ONE global hash at
//! `fleet:ready`. That is the first structure a million fleets contend on, and
//! the depth series this lane records is how growth outrunning drain becomes a
//! number rather than a stall somebody notices later.
//!
//! # The window is the submitters' window
//!
//! Its length is taken the moment the last submitter returns — before the
//! depth sampler is joined, before any histogram work. Under an abort the
//! submitters stop early and the window is that short; the first version
//! took the length after the sampler, which runs to the deadline, and would
//! have reported an aborted run's rate over a window it never used.

use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use afd_events::Steer;
use afd_redis::{ReadyIndex, Redis};
use tokio_util::sync::CancellationToken;

use crate::abort::Abort;
use crate::datastores::{Datastores, postgres_transactions, redis_calls};
use crate::error::{Error, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::seed::{
    self, BENCH_ACTOR, BENCH_REQUEST_JSON, ROWS_PER_FLEET, SEEDED_AT, SeededFleet,
};
use crate::lane::outcomes::Outcomes;
use crate::profile::{Parameter, Profile};
use crate::report::{DatastoreCost, DatastoreCosts, Fixture, Lane, Report, count};

/// Measurement key: how many steers the window appended in total.
const ACCEPTED: &str = "accepted";

/// Measurement key: appends the path refused.
const FAILURES: &str = "failures";

/// Measurement key: the fraction of everything tried that the path refused.
const ERROR_RATE: &str = "error_rate";

/// Measurement key: Redis commands each accepted steer cost.
const REDIS_CALLS_PER_STEER: &str = "redis_calls_per_steer";

/// Measurement key: Postgres transactions each accepted steer cost.
///
/// A RATIO, because the total is not zero and a reader deserves to see why:
/// `pg_stat_database` counts every transaction the database served in the
/// window, including the pool keeping its connections alive and this lane's
/// own two readings of the statistic. A few hundred-thousandths per steer is
/// that residue; the ingress path issuing one would read as 1.0.
const POSTGRES_TRANSACTIONS_PER_STEER: &str = "postgres_transactions_per_steer";

/// Series key: readiness-index depth, sampled through the run.
const READY_DEPTH: &str = "ready_depth";

/// How often the readiness index is sampled.
const DEPTH_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// Distinct fleets steers are spread across.
    pub fleets: u64,
    /// Submitters appending at once.
    pub concurrency: u64,
    /// The longest the window may run.
    pub window: Duration,
}

impl Parameters {
    /// Refuse anything outside the profile's bounds, before a connection opens.
    ///
    /// # Errors
    ///
    /// A cap, a floor, or a window under the warmup floor, each named.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Fleets, self.fleets)?;
        profile.check(Parameter::Concurrency, self.concurrency)?;
        profile.check_window(self.window)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, a datastore that would not answer, or a lost task.
pub async fn run(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    run_cancelled(
        profile,
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
/// The same failures as [`run`], plus [`Error::Cancelled`] while seeding.
pub async fn run_cancelled(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
    cancellation: CancellationToken,
) -> Result<Report> {
    parameters.admit(profile)?;
    let abort = Arc::new(Abort::with_token(
        profile.caps().abort_error_rate,
        cancellation,
    ));
    let tag = seed::placement_tag(prefix);
    let mut ledger = FixtureLedger::new();

    let mut fleets = Vec::new();
    for index in 0..parameters.fleets {
        if abort.fired() {
            return Err(Error::Cancelled);
        }
        fleets.push(
            seed::empty_fleet(
                &stores.database,
                &stores.queue,
                prefix,
                &tag,
                index,
                SEEDED_AT,
            )
            .await?,
        );
        ledger.created(ROWS_PER_FLEET);
    }

    let measured = submit(stores, &fleets, parameters, &abort).await?;

    let mut report = Report::new(Lane::Steer, profile);
    report.created = true;
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    report.parameter(Parameter::Concurrency.name(), parameters.concurrency);
    report.parameter(crate::knobs::WINDOW_VARIABLE, parameters.window.as_secs());
    measured.record(&mut report);
    report.abort = abort.recorded();
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// One window of concurrent appends, with the cost either side of it.
struct Submitted {
    outcomes: Outcomes,
    length: Duration,
    redis_calls: u64,
    transactions: u64,
    depth: Vec<u64>,
}

/// Each submitter's slice of the population: fleet id and the workspace that
/// owns it, so an appended entry carries the workspace of its own fleet.
type Slice = Vec<(String, String)>;

/// Drive every submitter concurrently, sampling the index while they run.
async fn submit(
    stores: &Datastores,
    fleets: &[SeededFleet],
    parameters: Parameters,
    abort: &Arc<Abort>,
) -> Result<Submitted> {
    // Every slice is built BEFORE the window opens, so partitioning the
    // population is not charged to the rate and every submitter starts on
    // the same instant rather than as its slice finishes.
    let slices = partition(fleets, parameters.concurrency);
    let stop = CancellationToken::new();
    let sampler = tokio::spawn(sample_depth(stores.queue.clone(), stop.clone()));
    let redis_before = redis_calls(&stores.queue).await?;
    let transactions_before = postgres_transactions(&stores.database).await?;
    let started = Instant::now();
    let deadline = started + parameters.window;

    let mut tasks = Vec::with_capacity(slices.len());
    for mine in slices {
        let steer = Steer::new(stores.queue.clone());
        let abort = Arc::clone(abort);
        tasks.push(tokio::spawn(async move {
            append_until(&steer, &mine, deadline, &abort).await
        }));
    }
    let mut outcomes = Outcomes::new()?;
    for task in tasks {
        let theirs = task
            .await
            .map_err(|_joined| Error::TaskLost { role: "submitter" })??;
        outcomes.absorb(&theirs)?;
    }
    // The window is the submitters', measured the instant they are all back.
    let length = started.elapsed();
    let redis_calls = redis_calls(&stores.queue)
        .await?
        .saturating_sub(redis_before);
    let transactions = postgres_transactions(&stores.database)
        .await?
        .saturating_sub(transactions_before);
    stop.cancel();
    let depth = sampler.await.map_err(|_joined| Error::TaskLost {
        role: "readiness sampler",
    })?;

    Ok(Submitted {
        outcomes,
        length,
        redis_calls,
        transactions,
        depth,
    })
}

/// Deal the population out to `concurrency` submitters, round-robin.
fn partition(fleets: &[SeededFleet], concurrency: u64) -> Vec<Slice> {
    let lanes = usize::try_from(concurrency).unwrap_or(1).max(1);
    let mut slices: Vec<Slice> = vec![Vec::new(); lanes];
    for (index, fleet) in fleets.iter().enumerate() {
        if let Some(slice) = slices.get_mut(index % lanes) {
            slice.push((fleet.fleet.clone(), fleet.workspace.clone()));
        }
    }
    slices
        .into_iter()
        .filter(|slice| !slice.is_empty())
        .collect()
}

/// Append to each fleet in turn until the deadline or the monitor fires.
async fn append_until(
    steer: &Steer,
    fleets: &[(String, String)],
    deadline: Instant,
    abort: &Abort,
) -> Result<Outcomes> {
    let mut outcomes = Outcomes::new()?;
    let token = abort.token();
    // A slice is never empty (`partition` drops empty ones), so cycling it
    // always yields; the deadline and the token are what end the loop.
    for (fleet, workspace) in fleets.iter().cycle() {
        if Instant::now() >= deadline || token.is_cancelled() {
            break;
        }
        let started = Instant::now();
        match steer
            .append(fleet, workspace, BENCH_ACTOR, BENCH_REQUEST_JSON)
            .await
        {
            Ok(_id) => {
                outcomes.succeeded(started.elapsed())?;
                abort.record(true);
            }
            Err(_refused) => {
                outcomes.failed();
                abort.record(false);
            }
        }
    }
    Ok(outcomes)
}

/// Sample the readiness index until told to stop.
///
/// A separate task rather than a reading taken by each submitter: the depth is
/// a property of the index over TIME, and a sample taken inside an append loop
/// would be a sample taken at whatever rate that loop happened to run. It
/// stops on the lane's signal, not on the deadline, so an aborted window does
/// not leave it running alone.
async fn sample_depth(queue: Redis, stop: CancellationToken) -> Vec<u64> {
    let index = ReadyIndex::new(queue);
    let mut series = Vec::new();
    while !stop.is_cancelled() {
        if let Ok(depth) = index.len().await {
            series.push(depth);
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(DEPTH_SAMPLE_INTERVAL) => {}
        }
    }
    series
}

impl Submitted {
    /// Write this window's numbers into the report.
    fn record(&self, report: &mut Report) {
        report.latency(self.length, &self.outcomes.latency);
        report.count(ACCEPTED, self.outcomes.successes);
        report.count(FAILURES, self.outcomes.failures);
        report.ratio(
            ERROR_RATE,
            self.outcomes.failures,
            self.outcomes.attempts() + self.outcomes.failures,
        );
        report.ratio(
            REDIS_CALLS_PER_STEER,
            self.redis_calls,
            self.outcomes.successes,
        );
        report.ratio(
            POSTGRES_TRANSACTIONS_PER_STEER,
            self.transactions,
            self.outcomes.successes,
        );
        for depth in &self.depth {
            report.series_value(
                READY_DEPTH,
                count(*depth),
                crate::report::Calculation::count(*depth),
            );
        }
        report.datastores = DatastoreCosts {
            redis: DatastoreCost {
                operations: self.redis_calls,
                time_ms: None,
            },
            postgres: DatastoreCost {
                operations: self.transactions,
                time_ms: None,
            },
        };
    }
}
