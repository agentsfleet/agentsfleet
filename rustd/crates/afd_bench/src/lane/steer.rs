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
//! So the Postgres number this lane reports is expected to be zero, and it is
//! read from `pg_stat_database` rather than assumed: a zero nobody measured is
//! indistinguishable from a measurement nobody took.
//!
//! # The readiness index is the interesting part
//!
//! Every steer, for every fleet, writes a field into ONE global hash at
//! `fleet:ready`. That is the first structure a million fleets contend on, and
//! the depth series this lane records is how growth outrunning drain becomes a
//! number rather than a stall somebody notices later.

use core::time::Duration;
use std::time::Instant;

use afd_events::Steer;
use afd_redis::{ReadyIndex, Redis};

use crate::abort::Abort;
use crate::datastores::{Datastores, postgres_transactions, redis_calls};
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::drive::Polled;
use crate::lane::lease::seed;
use crate::profile::{Parameter, Profile};
use crate::report::{
    DatastoreCost, Datastores as ReportDatastores, Fixture, Lane, Latency, Report,
};

/// Measurement key: how many steers the window appended in total.
const ACCEPTED: &str = "accepted";

/// Measurement key: Redis commands each accepted steer cost.
const REDIS_CALLS_PER_STEER: &str = "redis_calls_per_steer";

/// Measurement key: Postgres transactions each accepted steer cost.
///
/// Reported as a RATIO rather than only as a total, because the total is not
/// zero and a reader deserves to see why. `pg_stat_database` counts every
/// transaction the database served during the window, and the pool keeps its
/// own connections alive underneath a lane that never queries. The ratio is
/// what shows that residue for what it is: a few hundredths of a transaction
/// per steer is background noise, where the ingress path issuing one would
/// read as 1.0.
const POSTGRES_TRANSACTIONS_PER_STEER: &str = "postgres_transactions_per_steer";

/// Series key: readiness-index depth, sampled through the run.
const READY_DEPTH: &str = "ready_depth";

/// How often the readiness index is sampled.
///
/// Frequent enough that a run of a few seconds still produces a series with
/// shape, rare enough that the sampler is not itself a load generator against
/// the structure it is watching.
const DEPTH_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

/// The clock a seeded row is stamped with.
const SEEDED_AT: i64 = 1_767_225_600_000;

/// The body every submitted steer carries.
///
/// Generated, never echoed from a tenant row: a fixture built from real
/// request text would put customer data in a bench result (RULE PRI).
const REQUEST_JSON: &str = "{\"prompt\":\"bench steer\"}";

/// The actor every submitted steer records.
const ACTOR: &str = "steer:bench";

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
    /// Refuse anything above the profile's ceiling, before a connection opens.
    ///
    /// # Errors
    ///
    /// [`crate::Error::CapExceeded`] naming the cap and the profile.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Fleets, self.fleets)?;
        profile.check(Parameter::Concurrency, self.concurrency)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, or a datastore that would not answer.
pub async fn run(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    let abort = std::sync::Arc::new(Abort::new(profile.caps().abort_error_rate));
    let tag = seed::placement_tag(prefix);
    let mut ledger = FixtureLedger::new();

    let mut fleets = Vec::new();
    for index in 0..parameters.fleets {
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
        ledger.created(seed::ROWS_PER_FLEET);
    }

    let measured = submit(stores, &fleets, parameters, &abort).await?;

    let mut report = Report::new(Lane::Steer, profile);
    report.created = true;
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    report.parameter(Parameter::Concurrency.name(), parameters.concurrency);
    measured.record(&mut report);
    report.abort = abort.recorded();
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// One window of concurrent appends, with the cost either side of it.
struct Submitted {
    polled: Polled,
    elapsed: Duration,
    redis_calls: u64,
    transactions: u64,
    depth: Vec<f64>,
    latency: Latency,
}

/// Drive every submitter concurrently, sampling the index while they run.
async fn submit(
    stores: &Datastores,
    fleets: &[seed::SeededFleet],
    parameters: Parameters,
    abort: &std::sync::Arc<Abort>,
) -> Result<Submitted> {
    let redis_before = redis_calls(&stores.queue).await?;
    let transactions_before = postgres_transactions(&stores.database).await?;
    let deadline = Instant::now() + parameters.window;
    let started = Instant::now();

    let sampler = tokio::spawn(sample_depth(stores.queue.clone(), deadline));
    let mut tasks = Vec::new();
    for submitter in 0..parameters.concurrency {
        let steer = Steer::new(stores.queue.clone());
        // Each submitter walks its own slice of the population, so two
        // submitters are not serialised behind one stream's key.
        let mine: Vec<String> = fleets
            .iter()
            .enumerate()
            .filter(|(index, _fleet)| (*index as u64) % parameters.concurrency == submitter)
            .map(|(_index, fleet)| fleet.fleet.clone())
            .collect();
        let workspace = fleets.first().map(|fleet| fleet.workspace.clone());
        let abort = std::sync::Arc::clone(abort);
        tasks.push(tokio::spawn(async move {
            append_until(
                &steer,
                &mine,
                workspace.unwrap_or_default(),
                deadline,
                &abort,
            )
            .await
        }));
    }

    let mut polled = Polled::default();
    for task in tasks {
        polled.absorb(
            task.await
                .map_err(|_joined| crate::Error::RunnerTaskLost)??,
        );
    }
    let depth = sampler
        .await
        .map_err(|_joined| crate::Error::RunnerTaskLost)?;

    let mut latency = Latency::new()?;
    for duration in &polled.durations {
        latency.record(*duration)?;
    }
    Ok(Submitted {
        elapsed: started.elapsed(),
        redis_calls: redis_calls(&stores.queue)
            .await?
            .saturating_sub(redis_before),
        transactions: postgres_transactions(&stores.database)
            .await?
            .saturating_sub(transactions_before),
        depth,
        latency,
        polled,
    })
}

/// Append to each fleet in turn until the deadline.
async fn append_until(
    steer: &Steer,
    fleets: &[String],
    workspace: String,
    deadline: Instant,
    abort: &Abort,
) -> Result<Polled> {
    let mut polled = Polled::default();
    if fleets.is_empty() {
        return Ok(polled);
    }
    let token = abort.token();
    // Round-robin over the slice: cycling the iterator means the fleet is
    // always present, so there is no index to check and no arm for an empty
    // slice past the guard above.
    let mut round_robin = fleets.iter().cycle();
    while Instant::now() < deadline && !token.is_cancelled() {
        let Some(fleet) = round_robin.next() else {
            break;
        };
        let started = Instant::now();
        // A refused append is counted and reported through the monitor, not
        // propagated: the monitor decides when refusing often enough is the
        // end of the window.
        match steer.append(fleet, &workspace, ACTOR, REQUEST_JSON).await {
            Ok(_id) => {
                polled.durations.push(started.elapsed());
                polled.leases += 1;
                abort.record(true);
            }
            Err(_refused) => {
                polled.failures += 1;
                abort.record(false);
            }
        }
    }
    Ok(polled)
}

/// Sample the readiness index until the deadline.
///
/// A separate task rather than a reading taken by each submitter: the depth is
/// a property of the index over TIME, and a sample taken inside an append loop
/// would be a sample taken at whatever rate that loop happened to run.
async fn sample_depth(queue: Redis, deadline: Instant) -> Vec<f64> {
    let index = ReadyIndex::new(queue);
    let mut series = Vec::new();
    while Instant::now() < deadline {
        if let Ok(depth) = index.len().await {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a readiness depth past f64's exact range is not a deployment"
            )]
            series.push(depth as f64);
        }
        tokio::time::sleep(DEPTH_SAMPLE_INTERVAL).await;
    }
    series
}

impl Submitted {
    /// A cost divided across the steers that were accepted.
    fn per_steer(&self, total: u64) -> f64 {
        if self.polled.leases == 0 {
            return 0.0;
        }
        count(total) / count(self.polled.leases)
    }

    /// Write this window's numbers into the report.
    fn record(&self, report: &mut Report) {
        let seconds = self.elapsed.as_secs_f64();
        report.latency(seconds, &self.latency);
        report.measurement(ACCEPTED, count(self.polled.leases));
        // `rate_per_second` already carries accepted-per-second: every append
        // records exactly one latency sample, so the shared key and a
        // lane-specific one would be the same number under two names.
        report.measurement(REDIS_CALLS_PER_STEER, self.per_steer(self.redis_calls));
        report.measurement(
            POSTGRES_TRANSACTIONS_PER_STEER,
            self.per_steer(self.transactions),
        );
        report
            .series
            .insert(READY_DEPTH.to_owned(), self.depth.clone());
        report.datastores = ReportDatastores {
            redis: DatastoreCost {
                operations: self.redis_calls,
                time_ms: None,
            },
            // Expected to be zero, and read from the server rather than
            // assumed: the ingress path never reaches Postgres.
            postgres: DatastoreCost {
                operations: self.transactions,
                time_ms: None,
            },
        };
    }
}

/// A count as a ratio's operand.
fn count(value: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count past f64's exact range is not a run that finished"
    )]
    {
        value as f64
    }
}
