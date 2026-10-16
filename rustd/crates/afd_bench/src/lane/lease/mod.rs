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
//! # Idle cost is the drain's to measure
//!
//! This window leases and never reports, so every fleet it touched ends it
//! claimed and still marked: work in flight, not drained. What a deployment
//! holding no work costs a runner fleet is measured by [`drain`], which drives
//! the full lease and report verbs over a population of its own until the
//! lease path itself has cleared every mark, and polls on from there. This
//! lane once measured idle here too, over an index it had emptied by hand —
//! a number that described the hand, not the path.

pub mod drain;
pub mod drive;
pub mod seed;
mod window;

use core::time::Duration;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::Leases;

use self::seed::{ROWS_PER_FLEET, ROWS_PER_RUNNER, SEEDED_AT};
use self::window::Pollers;
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::instrument::LeaseInstrument;
use crate::profile::{Parameter, Profile};
use crate::report::{Fixture, Lane, Provenance, Report};

/// Parameter key: connections the pool may open, so a p95 is attributable.
const POOL_SIZE: &str = "pool_size";

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

    /// Refuse more runners than the pool has connections.
    ///
    /// # Errors
    ///
    /// `RunnersExceedPool` naming both numbers.
    pub fn fit(self, pool_size: u32) -> Result<()> {
        if self.runners > u64::from(pool_size) {
            return Err(ErrorKind::RunnersExceedPool {
                runners: self.runners,
                pool: pool_size,
            }
            .into());
        }
        Ok(())
    }

    /// An empty lease report carrying these parameters and the pool they ran on.
    fn report(self, profile: Profile, provenance: Provenance, pool_size: u32) -> Report {
        let mut report = Report::new(Lane::Lease, profile, provenance);
        report.created = true;
        report.parameter(Parameter::Fleets.name(), self.fleets);
        report.parameter(Parameter::Runners.name(), self.runners);
        report.parameter(POOL_SIZE, u64::from(pool_size));
        report
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
    provenance: Provenance,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    parameters.fit(stores.pool_size)?;
    let abort = Arc::new(Abort::new(profile.caps().abort_error_rate));
    let instrument = LeaseInstrument::install()?;
    let leases = Leases::new(
        stores.database.clone(),
        stores.queue.clone(),
        Entropy::new(),
    );
    let tag = seed::placement_tag(prefix);

    let mut ledger = FixtureLedger::new();
    let runners = populate(stores, prefix, &tag, parameters, &mut ledger).await?;
    let pollers = Pollers {
        instrument: &instrument,
        leases: &leases,
        stores,
        runners: &runners,
        abort: &abort,
    };

    let contended = pollers
        .measure(parameters.window, Some(parameters.fleets))
        .await?;
    let mut report = parameters.report(profile, provenance, stores.pool_size);
    contended.record(&mut report);
    report.abort = abort.recorded();
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// The drain, then the contended window, as one report: `make bench-lease`.
///
/// On a rig the run owns, the readiness index is emptied before anything is
/// seeded, so the idle cost measured is this run's. The drain goes FIRST. The contended window leases and never reports, so
/// its fleets end it claimed and marked for a claim's whole lifetime; a drain
/// after it would poll those marks, and its idle cost would be theirs. Each
/// population is its own, so the contended numbers stay comparable with
/// every earlier baseline either way.
///
/// # Errors
///
/// Whatever either population refused, the drain's first.
pub async fn run_both(
    profile: Profile,
    provenance: Provenance,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    if provenance.owned {
        drain::reset_readiness(stores).await?;
    }
    let mut drained = Report::new(Lane::Lease, profile, provenance.clone());
    drain::run(profile, parameters, stores, prefix, &mut drained).await?;
    let mut report = run(profile, provenance, parameters, stores, prefix).await?;
    report.measurements.append(&mut drained.measurements);
    report.fixture.created += drained.fixture.created;
    Ok(report)
}

/// Seed the population and enrol the runners that will poll it, answering
/// the runners.
async fn populate(
    stores: &Datastores,
    prefix: &RunPrefix,
    tag: &str,
    parameters: Parameters,
    ledger: &mut FixtureLedger,
) -> Result<Vec<Uuid7>> {
    for index in 0..parameters.fleets {
        seed::ready_fleet(
            &stores.database,
            &stores.queue,
            prefix,
            tag,
            index,
            SEEDED_AT,
        )
        .await?;
        // Rows, not fleets: the sweep counts rows and the two must agree.
        ledger.created(ROWS_PER_FLEET);
    }
    let mut runners = Vec::new();
    for index in 0..parameters.runners {
        let host = prefix.name(&format!("host-{index}"));
        runners.push(seed::runner(&stores.database, &host, tag, SEEDED_AT).await?);
        ledger.created(ROWS_PER_RUNNER);
    }
    Ok(runners)
}

#[cfg(test)]
mod tests;
