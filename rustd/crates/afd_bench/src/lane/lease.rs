//! What lease issuance sustains, and what each lease costs Postgres.
//!
//! # The shape of a run
//!
//! Seed K ready fleets and enrol R runners, then let all R poll the real
//! assignment pass at once until every fleet is leased or the window ends. The
//! rate is leases over the time it took to hand them out, which is the
//! question an operator asks: how long does this deployment take to issue the
//! work it has.
//!
//! A leased fleet is claimed and not leasable again, so the run ends when the
//! population is exhausted. Re-seeding mid-window would put ingress on the
//! same connections the thing under measurement is using, and the lane would
//! report the sum of two paths under the name of one.
//!
//! # Then a second, quieter window
//!
//! With every fleet leased and every mark cleared, the index is empty and the
//! pass returns before it touches Postgres. Polling there measures idle cost:
//! what a runner fleet costs a deployment holding no work. Multiplied by a
//! million fleets that is the standing bill for the current design.
//!
//! The idle window reports the index depth it actually saw. Its first run at
//! two hundred fleets found fifteen Postgres round trips per "idle" poll,
//! because something still held marks after the clear; a number with the
//! depth beside it says so, where a bare number would have been quoted.

pub mod drive;
pub mod seed;
mod window;

use core::time::Duration;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::Leases;
use afd_redis::ReadyIndex;
use tokio_util::sync::CancellationToken;

use self::seed::{ROWS_PER_FLEET, ROWS_PER_RUNNER, SEEDED_AT, SeededFleet};
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::{Error, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::instrument::LeaseInstrument;
use crate::profile::{Parameter, Profile};
use crate::report::{Fixture, Lane, Report};

/// Measurement key: polls issued per second, lease or miss.
const POLLS_PER_SECOND: &str = "polls_per_second";

/// Measurement key: how many leases the window issued in total.
const LEASES: &str = "leases";

/// Measurement key: polls the path refused.
const FAILURES: &str = "failures";

/// Measurement key: the fraction of everything tried that the path refused.
const ERROR_RATE: &str = "error_rate";

/// Measurement key: Postgres round trips the pass made per issued lease.
const ROUNDTRIPS_PER_LEASE: &str = "roundtrips_per_lease";

/// Measurement key: the fraction of polls that produced no work.
const WASTED_CLAIM_RATE: &str = "wasted_claim_rate";

/// Measurement key: whether the contended window ended at exhaustion (1) or
/// at its deadline (0), so a reader knows which the rate is over.
const EXHAUSTED: &str = "exhausted";

/// Measurement key: how many marks the readiness index held when the idle
/// window began. Zero is the only value under which the idle numbers mean
/// what their names say.
const IDLE_INDEX_DEPTH: &str = "idle_index_depth";

/// Measurement key: Postgres round trips one poll costs with nothing ready.
const IDLE_ROUNDTRIPS_PER_POLL: &str = "idle_roundtrips_per_poll";

/// Measurement key: Redis commands one poll costs with nothing ready.
const IDLE_REDIS_CALLS_PER_POLL: &str = "idle_redis_calls_per_poll";

/// Measurement key: how many polls the idle window managed.
const IDLE_POLLS: &str = "idle_polls";

/// Parameter key: connections the pool may open, so a p95 is attributable.
const POOL_SIZE: &str = "pool_size";

/// How long the idle window runs.
///
/// Short on purpose: idle cost is per-poll and does not need a long window to
/// resolve, and every second here is a second the contended measurement is not
/// using.
const IDLE_WINDOW: Duration = Duration::from_secs(2);

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
    /// Refuse anything outside the profile's bounds, before a connection opens.
    ///
    /// # Errors
    ///
    /// A cap, a floor, or a window under the warmup floor, each named.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Fleets, self.fleets)?;
        profile.check(Parameter::Runners, self.runners)?;
        profile.check_window(self.window)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, more runners than the pool has connections, a datastore
/// that would not answer, or a lost task. Never a slow result.
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
    if parameters.runners > u64::from(stores.pool_size) {
        return Err(Error::RunnersExceedPool {
            runners: parameters.runners,
            pool: stores.pool_size,
        });
    }
    let abort = Arc::new(Abort::with_token(
        profile.caps().abort_error_rate,
        cancellation,
    ));
    let instrument = LeaseInstrument::install()?;
    let leases = Leases::new(
        stores.database.clone(),
        stores.queue.clone(),
        Entropy::new(),
    );
    let tag = seed::placement_tag(prefix);

    let mut ledger = FixtureLedger::new();
    let (seeded, runners) = populate(stores, prefix, &tag, parameters, &abort, &mut ledger).await?;

    let contended = window::measure(
        &instrument,
        &leases,
        stores,
        &runners,
        parameters.window,
        Some(parameters.fleets),
        &abort,
    )
    .await?;

    let mut report = Report::new(Lane::Lease, profile);
    report.created = true;
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    report.parameter(Parameter::Runners.name(), parameters.runners);
    report.parameter(crate::knobs::WINDOW_VARIABLE, parameters.window.as_secs());
    report.parameter(POOL_SIZE, u64::from(stores.pool_size));
    contended.record(&mut report);

    // The idle window only means "idle" when the contended one finished on
    // its own terms. After an abort every runner would exit on entry and the
    // per-poll ratios would describe a window that never polled.
    if !abort.fired() {
        let depth = quiesce(&stores.queue, &seeded).await?;
        let idle = window::measure(
            &instrument,
            &leases,
            stores,
            &runners,
            IDLE_WINDOW,
            None,
            &abort,
        )
        .await?;
        idle.record_idle(&mut report, depth);
    }
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
    abort: &Abort,
    ledger: &mut FixtureLedger,
) -> Result<(Vec<SeededFleet>, Vec<Uuid7>)> {
    let mut seeded = Vec::new();
    for index in 0..parameters.fleets {
        if abort.fired() {
            return Err(Error::Cancelled);
        }
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
        // Rows, not fleets: the sweep counts rows and the two must agree.
        ledger.created(ROWS_PER_FLEET);
    }
    let mut runners = Vec::new();
    for index in 0..parameters.runners {
        if abort.fired() {
            return Err(Error::Cancelled);
        }
        let host = prefix.name(&format!("host-{index}"));
        runners.push(seed::runner(&stores.database, &host, tag, SEEDED_AT).await?);
        ledger.created(ROWS_PER_RUNNER);
    }
    Ok((seeded, runners))
}

/// Clear this run's readiness marks, answering how many the index still
/// holds afterwards — the depth the idle window will actually poll against.
async fn quiesce(queue: &afd_redis::Redis, seeded: &[SeededFleet]) -> Result<u64> {
    let ready = ReadyIndex::new(queue.clone());
    for fleet in seeded {
        ready.force_clear(&fleet.fleet).await?;
    }
    Ok(ready.len().await?)
}
