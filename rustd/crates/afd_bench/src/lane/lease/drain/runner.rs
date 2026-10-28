//! Drive the production lease and report paths for each benchmark runner.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_core::clock;
use afd_core::id::Uuid7;
use afd_fleet::lease::Plane;
use afd_wire::lease::{LeasePayload, LeaseResponse};
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use crate::abort::Abort;
use crate::error::{ErrorKind, Result};
use crate::report::Latency;

/// What every drain runner reports as its run's answer.
///
/// Generated, never a tenant's text, for the reason the seed's request body
/// gives (RULE PRI).
const RESPONSE: &str = "bench";

/// What the drain's runners share: when to stop, and how far they have got.
#[derive(Debug)]
pub(super) struct Shared {
    /// The window's end, whatever else happens.
    pub(super) deadline: Instant,
    /// Events every runner has reported so far.
    pub(super) reported: AtomicU64,
    /// Stop once `reported` reaches this; `None` polls to the deadline.
    pub(super) target: Option<u64>,
    /// The monitor every outcome is reported to.
    pub(super) abort: Arc<Abort>,
}

impl Shared {
    /// Whether this window should keep polling.
    fn open(&self) -> bool {
        Instant::now() < self.deadline
            && !self.abort.token().is_cancelled()
            && self
                .target
                .is_none_or(|target| self.reported.load(Ordering::Relaxed) < target)
    }
}

/// What one runner did in one window.
#[derive(Debug)]
pub(super) struct Tally {
    /// Polls made, lease or not.
    pub(super) polls: u64,
    /// Leases issued to this runner.
    pub(super) leased: u64,
    /// Polls or reports the path refused.
    pub(super) failures: u64,
    /// How long a poll that issued a lease took.
    pub(super) lease_latency: Latency,
    /// How long the report after it took.
    pub(super) report_latency: Latency,
}

impl Tally {
    /// An empty tally.
    fn new() -> Result<Self> {
        Ok(Self {
            polls: 0,
            leased: 0,
            failures: 0,
            lease_latency: Latency::new()?,
            report_latency: Latency::new()?,
        })
    }

    /// Fold another runner's tally into this one.
    ///
    /// # Errors
    ///
    /// When the two latency distributions will not merge.
    pub(super) fn absorb(&mut self, other: &Self) -> Result<()> {
        self.polls += other.polls;
        self.leased += other.leased;
        self.failures += other.failures;
        self.lease_latency.merge(&other.lease_latency)?;
        self.report_latency.merge(&other.report_latency)
    }
}

/// Poll and report until the window closes, answering what this runner did.
///
/// # Errors
///
/// A latency the histogram would not hold, or a lease answer that does not
/// parse. Never a refusal from the path, which is counted.
pub(super) async fn drive(plane: &Plane, runner: &Uuid7, shared: &Shared) -> Result<Tally> {
    let mut tally = Tally::new()?;
    while shared.open() {
        tally.polls += 1;
        if let Err(refused) = step(plane, runner, shared, &mut tally).await {
            if tally.failures == 0 {
                refusal(&refused);
            }
            tally.failures += 1;
            shared.abort.record(false);
        }
    }
    Ok(tally)
}

/// Keep each refusal's cause intact before the driver counts it.
async fn step(plane: &Plane, runner: &Uuid7, shared: &Shared, tally: &mut Tally) -> Result<()> {
    let started = Instant::now();
    let answer = plane.lease(runner, &[], false, clock::now()).await?;
    let took = started.elapsed();
    shared.abort.record(true);
    let response: LeaseResponse<'_> =
        serde_json::from_str(&answer).map_err(|source| ErrorKind::LeaseUnreadable { source })?;
    let Some(afd_wire::lease::TaggedLease::Fleet(lease)) = response.lease else {
        return Ok(());
    };
    tally.leased += 1;
    tally.lease_latency.record(took)?;
    let reporting = Instant::now();
    plane
        .report(runner, &processed(&lease), clock::now())
        .await?;
    tally.report_latency.record(reporting.elapsed())?;
    shared.reported.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// The first refusal per runner is enough to diagnose a failed measurement.
fn refusal(error: &crate::error::Error) {
    // the benchmark has no subscriber; its first refusal belongs on the same stderr as the final run failure.
    eprintln!("drain runner refused: {error}");
    let mut cause = core::error::Error::source(error);
    while let Some(reason) = cause {
        // preserve the cause chain for a benchmark failure without printing any fixture request.
        eprintln!("  caused by: {reason}");
        cause = reason.source();
    }
}

/// The report a runner that ran the event and answered sends back.
fn processed<'a>(lease: &'a LeasePayload<'a>) -> ReportRequest<'a> {
    ReportRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        event_id: Cow::Borrowed(&lease.event.event_id),
        fencing_token: lease.fencing_token,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE),
        tokens: 0,
        requests: afd_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 0,
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event.event_id),
            last_response: Cow::Borrowed(RESPONSE),
        },
        tool_calls: None,
        held_until: None,
    }
}
