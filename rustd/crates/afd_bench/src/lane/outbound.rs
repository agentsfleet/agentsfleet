//! What one delivery worker sustains, and what a slow destination costs the
//! jobs queued behind it.
//!
//! # The shape of a run
//!
//! Queue N jobs across D destinations, then start the real `Worker` and let it
//! drain them. The worker is one task reading one entry at a time and awaiting
//! its retry ladder inline, so every job's delivery latency includes the time
//! every earlier job spent — which is exactly the head-of-line cost this lane
//! exists to measure. Nothing here changes that shape; the numbers are what
//! say whether it should change.
//!
//! # The clock starts when the worker does
//!
//! Every job's latency is measured from the LATER of its enqueue and the
//! worker's start. The reader's connect and the enqueue loop happen before
//! that instant, and the first version charged them to job zero — a head-of-
//! line number that scaled with how many jobs were queued behind it.
//!
//! # The window ends at the last settlement, not the next tick
//!
//! The drain waits on the poster's own signal and takes the window's length
//! from the last attempt it made, so a four-job run is not reported over the
//! fifty milliseconds a polling loop happened to sleep.
//!
//! # A drain that did not finish says so
//!
//! The worker turns a dead datastore into pause-and-retry and never returns
//! an error, so a window that reached its deadline with jobs unsettled is the
//! only signal there is. It is recorded as an abort, and the rate is over the
//! jobs that did settle.

pub mod poster;
mod record;

use core::time::Duration;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use afd_outbound::{Posters, Worker};
use afd_redis::{OutboundJob, OutboundQueue, OutboundReader, outbound_consumer};
use tokio_util::sync::CancellationToken;

use self::poster::{Behaviour, Scripted};
use self::record::{Drained, record, window_end};
use crate::datastores::{Datastores, postgres_transactions, redis_calls};
use crate::error::{Error, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::knobs::{RETRYABLE_FRACTION_VARIABLE, SLOW_FRACTION_VARIABLE};
use crate::lane::sweep;
use crate::profile::{Parameter, Profile};
use crate::report::{Fixture, Lane, Report};

/// A healthy vendor's answer time. One millisecond is well under the ladder's
/// first rung, so the fast population cannot be confused with a retry.
const FAST_ANSWER: Duration = Duration::from_millis(1);

/// A slow vendor's answer time: long enough that the jobs behind it visibly
/// wait, short enough that a window of seconds still drains.
const SLOW_ANSWER: Duration = Duration::from_millis(250);

/// How many destinations the jobs are spread over.
const DESTINATIONS: u64 = 16;

/// The provider every queued job names, so the worker routes it to the one
/// poster this build ships.
const PROVIDER: &str = "slack";

/// A generated answer; never a tenant's text (RULE PRI).
const ANSWER: &str = "bench answer";

/// What the lost-task refusal calls the worker.
const WORKER_ROLE: &str = "delivery worker";

/// How long to wait for the worker to stop after cancellation.
const STOP_GRACE: Duration = Duration::from_secs(6);

/// The longest the drain waits for a settlement before re-checking the
/// deadline; a bound on how late a deadline is noticed, not a sampling rate.
const SETTLEMENT_WAIT: Duration = Duration::from_millis(250);

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// Jobs to queue.
    pub jobs: u64,
    /// Fraction of destinations scripted slow, in `0..=1`.
    pub slow_fraction: f64,
    /// Fraction of destinations scripted retryable, in `0..=1`.
    pub retryable_fraction: f64,
    /// The longest the drain may run.
    pub window: Duration,
}

impl Parameters {
    /// Refuse anything outside the profile's bounds, before a connection opens.
    ///
    /// # Errors
    ///
    /// A cap, a floor, or a window under the warmup floor, each named.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Jobs, self.jobs)?;
        profile.check_window(self.window)
    }
}

/// Run the lane and return the report it measured.
///
/// The run removes the entries it queued before returning, by the ids it was
/// handed; the caller's prefix sweep is the fallback for a run that failed
/// before it could.
///
/// # Errors
///
/// A cap refusal, a datastore that would not answer, or a lost worker.
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
/// The same failures as [`run`], plus [`Error::Cancelled`] while queuing or
/// draining synthetic jobs.
pub async fn run_cancelled(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
    cancellation: CancellationToken,
) -> Result<Report> {
    parameters.admit(profile)?;
    let queue = OutboundQueue::new(stores.queue.clone());
    queue.ensure_group().await?;

    let behaviours = script(prefix, parameters);
    let poster = Scripted::owned(
        behaviours.clone(),
        FAST_ANSWER,
        SLOW_ANSWER,
        cancellation.clone(),
        prefix,
    );
    let destinations: Vec<&String> = behaviours.keys().collect();

    let mut ledger = FixtureLedger::new();
    let mut queued_at = HashMap::new();
    for (index, destination) in (0..parameters.jobs).zip(destinations.iter().cycle()) {
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let id = queue
            .enqueue(OutboundJob {
                provider: PROVIDER,
                workspace_id: prefix.as_str(),
                fleet_id: destination,
                event_id: &prefix.name(&format!("event-{index}")),
                answer: ANSWER,
            })
            .await?;
        queued_at.insert(id.as_str().to_owned(), Instant::now());
        ledger.created(1);
    }

    let drained = drain(stores, queue, poster.clone(), parameters, &cancellation).await?;
    let ids: Vec<String> = queued_at.keys().cloned().collect();
    ledger.swept(sweep::outbound_entries(&stores.queue, &ids).await?);

    let mut report = Report::new(Lane::Outbound, profile);
    report.created = true;
    report.parameter(Parameter::Jobs.name(), parameters.jobs);
    report.parameter(crate::knobs::WINDOW_VARIABLE, parameters.window.as_secs());
    report.parameter(
        SLOW_FRACTION_VARIABLE,
        fraction_of(DESTINATIONS, parameters.slow_fraction),
    );
    report.parameter(
        RETRYABLE_FRACTION_VARIABLE,
        fraction_of(DESTINATIONS, parameters.retryable_fraction),
    );
    record(
        &mut report,
        &drained,
        &poster,
        &queued_at,
        &behaviours,
        parameters.jobs,
    )?;
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// Assign a behaviour to each destination by the requested fractions.
///
/// A `BTreeMap`, so the order jobs are dealt to destinations — and therefore
/// which position in each cycle the slow one occupies — is a function of the
/// parameters and not of a hash seed that changes per process.
pub(crate) fn script(prefix: &RunPrefix, parameters: Parameters) -> BTreeMap<String, Behaviour> {
    let slow = fraction_of(DESTINATIONS, parameters.slow_fraction);
    let retryable = fraction_of(DESTINATIONS, parameters.retryable_fraction);
    (0..DESTINATIONS)
        .map(|index| {
            let behaviour = if index < slow {
                Behaviour::Slow
            } else if index < slow + retryable {
                Behaviour::Retryable
            } else {
                Behaviour::Fast
            };
            (prefix.name(&format!("destination-{index:02}")), behaviour)
        })
        .collect()
}

/// How many of `total` a fraction selects, rounded down and clamped.
pub(crate) fn fraction_of(total: u64, fraction: f64) -> u64 {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sixteen destinations times a clamped fraction is a small non-negative integer"
    )]
    {
        (total as f64 * fraction.clamp(0.0, 1.0)).floor() as u64
    }
}

/// Start the real worker, wait for it to settle every job or the deadline,
/// stop it.
async fn drain(
    stores: &Datastores,
    queue: OutboundQueue,
    poster: Scripted,
    parameters: Parameters,
    cancellation: &CancellationToken,
) -> Result<Drained> {
    let reader = OutboundReader::new(
        stores.dedicated(afd_outbound::LONGEST_PARK).await?,
        outbound_consumer(),
    );
    let redis_before = redis_calls(&stores.queue).await?;
    let transactions_before = postgres_transactions(&stores.database).await?;
    let token = cancellation.child_token();
    let started = Instant::now();
    let worker = tokio::spawn(
        Worker::new(
            reader,
            queue,
            Posters {
                slack: poster.clone(),
            },
        )
        .run(token.clone()),
    );

    let deadline = started + parameters.window;
    while Instant::now() < deadline
        && poster.settled() < parameters.jobs
        && !cancellation.is_cancelled()
    {
        poster.settlement(SETTLEMENT_WAIT).await;
    }
    let settled = poster.settled();
    token.cancel();
    tokio::time::timeout(STOP_GRACE, worker)
        .await
        .map_err(|_elapsed| Error::TaskLost { role: WORKER_ROLE })?
        .map_err(|_joined| Error::TaskLost { role: WORKER_ROLE })?;
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }

    // The window ends at the last answer this run's jobs received, not at the
    // last attempt's start; the deadline stands in only when nothing was asked.
    let ended = window_end(&poster.seen(), deadline);

    Ok(Drained {
        started,
        ended,
        settled,
        redis_calls: redis_calls(&stores.queue)
            .await?
            .saturating_sub(redis_before),
        transactions: postgres_transactions(&stores.database)
            .await?
            .saturating_sub(transactions_before),
    })
}

#[cfg(test)]
mod tests;
