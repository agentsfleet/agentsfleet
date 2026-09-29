//! One simulated runner inside the drain: poll, run nothing, report.
//!
//! # What one iteration is
//!
//! `Plane::lease` — the whole verb the runner route serves, from the readiness
//! peek to the serialized answer — then, when it issued work,
//! `Plane::report` with a processed outcome. The run in between is empty on
//! purpose: the drain measures what the SCHEDULER and the ledger cost per
//! event, and a sleep standing in for a model call would only lengthen the
//! window those costs are divided over.
//!
//! # A refusal is counted, never propagated
//!
//! As in the contended window: a datastore that would not answer is an
//! outcome to count and hand to the abort monitor, not a reason to stop the
//! other runners mid-drain.

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
        let started = Instant::now();
        let Ok(answer) = plane.lease(runner, false, clock::now()).await else {
            tally.failures += 1;
            shared.abort.record(false);
            continue;
        };
        let took = started.elapsed();
        shared.abort.record(true);
        let response: LeaseResponse<'_> = serde_json::from_str(&answer)
            .map_err(|source| ErrorKind::LeaseUnreadable { source })?;
        let Some(lease) = response.lease else {
            continue;
        };
        tally.leased += 1;
        tally.lease_latency.record(took)?;
        let reporting = Instant::now();
        if plane
            .report(runner, &processed(&lease), clock::now())
            .await
            .is_ok()
        {
            tally.report_latency.record(reporting.elapsed())?;
            shared.reported.fetch_add(1, Ordering::Relaxed);
        } else {
            tally.failures += 1;
            shared.abort.record(false);
        }
    }
    Ok(tally)
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
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 0,
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event.event_id),
            last_response: Cow::Borrowed(RESPONSE),
        },
    }
}
