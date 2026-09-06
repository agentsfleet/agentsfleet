//! What lease issuance sustains, and what each lease costs Postgres.
//!
//! # The shape of a run
//!
//! Seed K ready fleets and enrol R runners, then let all R poll the real
//! assignment pass at once until every fleet is leased or the window ends. The
//! rate is leases over elapsed, which is the question an operator asks: how
//! long does it take this deployment to hand out the work it has.
//!
//! A leased fleet is claimed and not leasable again, so the run ends when the
//! population is exhausted. That is deliberate — re-seeding mid-window would
//! put ingress on the same connections the thing under measurement is using,
//! and the lane would report the sum of two paths under the name of one.
//!
//! # Then a second, quieter window
//!
//! With every fleet leased, the index is empty and the pass returns before it
//! touches Postgres. Polling there measures idle cost: what a runner fleet
//! costs a deployment holding no work. Multiplied by a million fleets that is
//! the standing bill for the current design, and it is the number this lane
//! exists to produce.

pub mod drive;
pub mod seed;

use core::time::Duration;
use std::time::Instant;

use afd_crypto::entropy::Entropy;
use afd_fleet::lease::Leases;

use crate::abort::Abort;
use crate::datastores::{Datastores, redis_calls};
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::instrument::LeaseInstrument;
use crate::profile::{Parameter, Profile};
use crate::report::{
    DatastoreCost, Datastores as ReportDatastores, Fixture, Lane, Latency, RATE_PER_SECOND, Report,
};

/// Measurement key: polls issued per second, lease or miss.
const POLLS_PER_SECOND: &str = "polls_per_second";

/// Measurement key: how many leases the window issued in total.
const LEASES: &str = "leases";

/// Measurement key: Postgres round trips the pass made per issued lease.
const ROUNDTRIPS_PER_LEASE: &str = "roundtrips_per_lease";

/// Measurement key: the fraction of polls that produced no work.
const WASTED_CLAIM_RATE: &str = "wasted_claim_rate";

/// Measurement key: Postgres round trips one poll costs with nothing ready.
const IDLE_ROUNDTRIPS_PER_POLL: &str = "idle_roundtrips_per_poll";

/// Measurement key: Redis commands one poll costs with nothing ready.
const IDLE_REDIS_CALLS_PER_POLL: &str = "idle_redis_calls_per_poll";

/// Measurement key: how many polls the idle window managed.
const IDLE_POLLS: &str = "idle_polls";

/// How long the idle window runs.
///
/// Short on purpose: idle cost is per-poll and does not need a long window to
/// resolve, and every second here is a second the contended measurement is not
/// using.
const IDLE_WINDOW: Duration = Duration::from_secs(2);

/// The clock a seeded row is stamped with, and the instant a poll is given.
///
/// A fixed past instant rather than "now": the candidate query orders by
/// enrolment, and a population seeded across a moving clock would order by the
/// accident of how long seeding took.
const SEEDED_AT: i64 = 1_767_225_600_000;

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// Ready fleets to seed.
    pub fleets: u64,
    /// Runners polling at once.
    pub runners: u64,
    /// The longest the contended window may run.
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
        profile.check(Parameter::Runners, self.runners)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, a datastore that would not answer, or a lease path that
/// faulted. Never a slow result: slowness is the output.
pub async fn run(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    let abort = std::sync::Arc::new(Abort::new(profile.caps().abort_error_rate));
    let instrument = LeaseInstrument::install()?;
    let leases = Leases::new(
        stores.database.clone(),
        stores.queue.clone(),
        Entropy::new(),
    );
    let tag = seed::placement_tag(prefix);

    let mut ledger = FixtureLedger::new();
    let (seeded, runners) = populate(stores, prefix, &tag, parameters, &mut ledger).await?;

    let contended = measure(
        &instrument,
        &leases,
        &stores.queue,
        &runners,
        parameters.window,
        Some(parameters.fleets),
        &abort,
    )
    .await?;
    // Emptying the index is what MAKES the next window idle. Issuing a lease
    // claims a fleet but leaves its readiness mark standing -- the mark is
    // ingress's to clear, and ingress is not running here -- so without this
    // every "idle" poll still peeks a full index, runs the candidate query and
    // reports a Postgres cost that has nothing to do with being idle. The
    // first run of this lane reported 41 round trips per idle poll for exactly
    // that reason.
    quiesce(&stores.queue, &seeded).await?;
    let idle = measure(
        &instrument,
        &leases,
        &stores.queue,
        &runners,
        IDLE_WINDOW,
        None,
        &abort,
    )
    .await?;

    let mut report = Report::new(Lane::Lease, profile);
    report.created = true;
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    report.parameter(Parameter::Runners.name(), parameters.runners);
    contended.record(&mut report, &instrument)?;
    idle.record_idle(&mut report);
    report.abort = abort.recorded();
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// Seed the population and enrol the runners that will poll it.
async fn populate(
    stores: &Datastores,
    prefix: &RunPrefix,
    tag: &str,
    parameters: Parameters,
    ledger: &mut FixtureLedger,
) -> Result<(Vec<seed::SeededFleet>, Vec<afd_core::id::Uuid7>)> {
    let mut seeded = Vec::new();
    for index in 0..parameters.fleets {
        seeded.push(
            seed::ready_fleet(
                &stores.database,
                &stores.queue,
                prefix,
                tag,
                index,
                SEEDED_AT,
            )
            .await?,
        );
        // THREE rows per fleet -- tenant, workspace, fleet -- because the sweep
        // counts rows deleted and the result file compares the two numbers. A
        // ledger counting fleets against a sweep counting rows would report a
        // clean run as a leak, or a leak as clean.
        ledger.created(seed::ROWS_PER_FLEET);
    }
    let mut runners = Vec::new();
    for index in 0..parameters.runners {
        let host = prefix.name(&format!("host-{index}"));
        runners.push(seed::runner(&stores.database, &host, tag, SEEDED_AT).await?);
        ledger.created(seed::ROWS_PER_RUNNER);
    }
    Ok((seeded, runners))
}

/// Clear this run's readiness marks, so the next window measures an idle poll.
async fn quiesce(queue: &afd_redis::Redis, seeded: &[seed::SeededFleet]) -> Result<()> {
    let ready = afd_redis::ReadyIndex::new(queue.clone());
    for fleet in seeded {
        ready.force_clear(&fleet.fleet).await?;
    }
    Ok(())
}

/// One window: every runner polling at once, with the cost either side of it.
struct Window {
    polled: drive::Polled,
    elapsed: Duration,
    counters: crate::instrument::PollCounters,
    redis_calls: u64,
}

/// Drive every runner concurrently for `window`, measuring what it cost.
async fn measure(
    instrument: &LeaseInstrument,
    leases: &Leases,
    queue: &afd_redis::Redis,
    runners: &[afd_core::id::Uuid7],
    window: Duration,
    stop_after: Option<u64>,
    abort: &std::sync::Arc<Abort>,
) -> Result<Window> {
    // The instrument is the CALLER's. Installing a second one here read zero
    // round trips off a provider nothing records into: `producers::install`
    // writes a `OnceLock`, so the producers stay bound to whichever provider
    // installed first and a later one collects an empty set forever.
    let before = instrument.read()?;
    let redis_before = redis_calls(queue).await?;
    let deadline = Instant::now() + window;
    let started = Instant::now();

    let mut tasks = Vec::with_capacity(runners.len());
    for runner in runners {
        let leases = leases.clone();
        let runner = runner.clone();
        let abort = std::sync::Arc::clone(abort);
        // Per runner, because the thing under measurement is what happens when
        // R of them reach the same readiness index at the same instant.
        tasks.push(tokio::spawn(async move {
            drive::poll_until(&leases, &runner, deadline, stop_after, &abort).await
        }));
    }

    let mut polled = drive::Polled::default();
    for task in tasks {
        polled.absorb(
            task.await
                .map_err(|_joined| crate::Error::RunnerTaskLost)??,
        );
    }

    Ok(Window {
        polled,
        elapsed: started.elapsed(),
        counters: instrument.read()?.since(before),
        redis_calls: redis_calls(queue).await?.saturating_sub(redis_before),
    })
}

impl Window {
    /// Write the contended window's numbers into the report.
    fn record(&self, report: &mut Report, instrument: &LeaseInstrument) -> Result<()> {
        let _ = instrument;
        let mut latency = Latency::new()?;
        for duration in &self.polled.durations {
            latency.record(*duration)?;
        }
        report.latency(self.elapsed.as_secs_f64(), &latency);
        // `Report::latency` reports SAMPLES per second, and every poll records
        // a sample. For this lane the headline is leases per second: a runner
        // fleet polling furiously and issuing nothing is the failure mode, not
        // the throughput. Polls per second stays, under its own name.
        let seconds = self.elapsed.as_secs_f64();
        report.measurement(POLLS_PER_SECOND, ratio(self.polled.polls(), seconds));
        report.measurement(RATE_PER_SECOND, ratio(self.polled.leases, seconds));
        report.measurement(LEASES, polls_as_f64(self.polled.leases));
        report.measurement(WASTED_CLAIM_RATE, self.polled.wasted_fraction());
        report.measurement(ROUNDTRIPS_PER_LEASE, self.roundtrips_per_lease());
        report.datastores = ReportDatastores {
            // No `time_ms`: this lane times the POLL end to end, which is
            // already the p95, and splitting that between the two datastores
            // would need a timer inside the pass rather than around it.
            redis: DatastoreCost {
                operations: self.redis_calls,
                time_ms: None,
            },
            postgres: DatastoreCost {
                operations: self.counters.roundtrips,
                time_ms: None,
            },
        };
        Ok(())
    }

    /// Write the idle window's numbers, which are per-poll rather than a rate.
    fn record_idle(&self, report: &mut Report) {
        let polls = self.polled.polls();
        report.measurement(IDLE_POLLS, polls_as_f64(polls));
        report.measurement(
            IDLE_ROUNDTRIPS_PER_POLL,
            self.counters.roundtrips_per_poll(),
        );
        report.measurement(
            IDLE_REDIS_CALLS_PER_POLL,
            if polls == 0 {
                0.0
            } else {
                polls_as_f64(self.redis_calls) / polls_as_f64(polls)
            },
        );
    }

    /// Postgres round trips the pass made for each lease it issued.
    fn roundtrips_per_lease(&self) -> f64 {
        if self.polled.leases == 0 {
            return 0.0;
        }
        polls_as_f64(self.counters.roundtrips) / polls_as_f64(self.polled.leases)
    }
}

/// A count over a span, or zero when the span is empty.
fn ratio(count: u64, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    polls_as_f64(count) / seconds
}

/// A count as a ratio's operand.
fn polls_as_f64(count: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count past f64's exact range is not a run that finished"
    )]
    {
        count as f64
    }
}
