//! A paced burst of steers to fleets whose runners are busy (`M241_001` §5).
//!
//! Every fleet receives `BENCH_RATE` steers a second through the production
//! intake (`Steer::append`) for the window, while the drain's runners hold
//! each lease for a whole turn before reporting. Today each steer is a run of
//! its own, so messages queue behind the run in flight; the burst measures how
//! long one waits, on the bench's own clock from its append returning to a
//! runner receiving its lease. That queue is what milestone 2's folding
//! removes.

mod pace;

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use afd_admission::Admissions;
use afd_crypto::entropy::Entropy;
use afd_events::Steer;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub(in crate::lane::lease::drain) use self::pace::Pickups;
use self::pace::{Appends, pace};
use super::turn::Turn;
use super::window::window;
use super::{platform, stage};
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::joined;
use crate::lane::lease::Parameters;
use crate::lane::lease::seed::{BENCH_ACTOR, BENCH_REQUEST_JSON};
use crate::profile::{Parameter, Profile};
use crate::report::{Lane, Latency, Provenance, Report, count, latency};

/// Measurement key: steers the intake accepted.
const ACCEPTED: &str = "burst_accepted";
/// Measurement key: steers the intake refused.
const REFUSED: &str = "burst_refused";
/// Measurement key: accepted steers no runner had leased when the window closed.
const UNLEASED: &str = "burst_unleased";
/// Measurement key: how long an accepted append took, at the tail.
const APPEND_P95_MS: &str = "burst_append_p95_ms";
/// Measurement key: append returned → lease received, median.
const PICKUP_P50_MS: &str = "burst_pickup_p50_ms";
/// Measurement key: the same, at p95.
const PICKUP_P95_MS: &str = "burst_pickup_p95_ms";
/// Measurement key: the same, at p99.
const PICKUP_P99_MS: &str = "burst_pickup_p99_ms";
/// Measurement key: the deepest unsettled backlog sampled during the window.
const BACKLOG_PEAK: &str = "burst_backlog_peak";

/// How often the backlog is sampled.
const BACKLOG_EVERY: Duration = Duration::from_millis(500);

/// Who a join failure names.
const PACER_ROLE: &str = "pacer";

/// What the burst asks for beyond the lease lane's own parameters.
#[derive(Debug, Clone, Copy)]
pub struct Burst {
    /// Steers a second each fleet receives.
    pub rate: u64,
}

/// Run one burst and answer its report.
///
/// # Errors
///
/// A refused parameter, whatever staging or the datastores refused, and the
/// platform rows' release.
pub async fn run(
    profile: Profile,
    provenance: Provenance,
    parameters: Parameters,
    burst: Burst,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    profile.check(Parameter::Rate, burst.rate)?;
    let mut report = Report::new(Lane::LeaseBurst, profile, provenance);
    report.parameter(Parameter::Rate.name(), burst.rate);
    let mut ledger = FixtureLedger::new();
    let staged = stage::stage(stores, prefix, parameters, &mut ledger).await;
    report.fixture.created += ledger.created_count();
    let measured = match staged {
        Ok(staged) => measure(profile, parameters, burst, stores, &staged, &mut report).await,
        Err(refused) => Err(refused),
    };
    let released = platform::release(stores, stage::INDEX_BASE).await;
    measured.and(released)?;
    Ok(report)
}

/// Pace steers into the staged fleets while the runners drain them.
async fn measure(
    profile: Profile,
    parameters: Parameters,
    burst: Burst,
    stores: &Datastores,
    staged: &stage::Staged,
    report: &mut Report,
) -> Result<()> {
    let plane = staged.plane(stores);
    let abort = Arc::new(Abort::new(profile.caps().abort_error_rate));
    let pickups = Arc::new(Pickups::new()?);
    let appends = Arc::new(Appends::new()?);
    let stop = CancellationToken::new();
    let sampler = tokio::spawn(peak_backlog(admissions(stores), stop.clone()));

    let mut pacers = Vec::with_capacity(staged.fleets.len());
    for fleet in &staged.fleets {
        let steer = Arc::new(Steer::new(admissions(stores)));
        let target = Arc::new((fleet.fleet.clone(), fleet.workspace.clone()));
        let (pickups, appends) = (Arc::clone(&pickups), Arc::clone(&appends));
        pacers.push(tokio::spawn(async move {
            pace(burst.rate, parameters.window, || {
                let (steer, target) = (Arc::clone(&steer), Arc::clone(&target));
                let (pickups, appends) = (Arc::clone(&pickups), Arc::clone(&appends));
                async move { append_one(&steer, &target, &pickups, &appends).await }
            })
            .await
        }));
    }
    window(
        &plane,
        stores,
        &staged.runners,
        parameters.window,
        None,
        Turn::held(parameters.turn),
        Some(Arc::clone(&pickups)),
        &abort,
    )
    .await?;
    for pacer in pacers {
        joined(pacer.await, PACER_ROLE)?;
    }
    stop.cancel();
    let peak = joined(sampler.await, PACER_ROLE)?;

    let counted = Counted {
        accepted: appends.accepted.load(Ordering::Relaxed),
        refused: appends.refused.load(Ordering::Relaxed),
        leased: pickups.leased_count(),
        backlog_peak: peak,
    };
    record(
        report,
        counted,
        &*appends.took.lock().await,
        &pickups.waits().await,
    );
    Ok(())
}

/// Append one steer and remember when it returned.
async fn append_one(
    steer: &Steer,
    (fleet, workspace): &(String, String),
    pickups: &Pickups,
    appends: &Appends,
) {
    let started = Instant::now();
    match steer
        .append(fleet, workspace, BENCH_ACTOR, BENCH_REQUEST_JSON, None)
        .await
    {
        Ok(steered) => {
            let returned = Instant::now();
            appends.accepted.fetch_add(1, Ordering::Relaxed);
            // A histogram that will not hold one append's time loses that one
            // sample, never the steer: the pickup is still timed below.
            drop(appends.took.lock().await.record(returned - started));
            pickups.appended(steered.event_id, returned).await;
        }
        Err(_refused) => {
            appends.refused.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The intake over the lane's pool and queue.
fn admissions(stores: &Datastores) -> Admissions {
    Admissions::new(
        stores.database.clone(),
        stores.queue.clone(),
        Entropy::new(),
    )
}

/// Sample the unsettled backlog until told to stop, answering its peak.
async fn peak_backlog(admissions: Admissions, stop: CancellationToken) -> u64 {
    let mut peak = 0;
    while !stop.is_cancelled() {
        if let Ok(backlog) = admissions.backlog().await {
            peak = peak.max(backlog.rows);
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(BACKLOG_EVERY) => {}
        }
    }
    peak
}

/// The burst's counts, apart from its latencies.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Counted {
    /// Steers the intake accepted.
    pub(super) accepted: u64,
    /// Steers the intake refused.
    pub(super) refused: u64,
    /// Accepted steers a runner leased.
    pub(super) leased: u64,
    /// The deepest backlog sampled.
    pub(super) backlog_peak: u64,
}

/// Write the burst's numbers. A refused steer has no pickup, so refusals are
/// counted apart and never reach a latency; an accepted steer still waiting
/// when the window closed is counted as unleased rather than dropped.
pub(super) fn record(report: &mut Report, counted: Counted, appended: &Latency, waits: &Latency) {
    report.measurement(ACCEPTED, count(counted.accepted));
    report.measurement(REFUSED, count(counted.refused));
    report.measurement(
        UNLEASED,
        count(counted.accepted.saturating_sub(counted.leased)),
    );
    report.measurement(BACKLOG_PEAK, count(counted.backlog_peak));
    if !appended.is_empty() {
        report.measurement(APPEND_P95_MS, appended.quantile_ms(latency::P95));
    }
    if !waits.is_empty() {
        report.measurement(PICKUP_P50_MS, waits.quantile_ms(latency::P50));
        report.measurement(PICKUP_P95_MS, waits.quantile_ms(latency::P95));
        report.measurement(PICKUP_P99_MS, waits.quantile_ms(latency::P99));
    }
}

/// The line the make target prints: what the rubric and a reader look for.
#[must_use]
pub fn summary(report: &Report) -> String {
    [
        ACCEPTED,
        REFUSED,
        UNLEASED,
        PICKUP_P95_MS,
        PICKUP_P99_MS,
        BACKLOG_PEAK,
    ]
    .iter()
    .filter_map(|key| {
        report
            .measurements
            .get(*key)
            .map(|value| format!("{key}={value}"))
    })
    .collect::<Vec<_>>()
    .join(" ")
}

#[cfg(test)]
mod tests;
