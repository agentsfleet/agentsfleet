//! Drive the production lease and report paths for each benchmark runner.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::native;
use af_wire::activity::{ActivityFrame, ActivityRequest, FleetResponseChunk, StreamTextKind};
use af_wire::fold::FoldRequest;
use af_wire::lease::LeasePayload;
use af_wire::report::{Outcome, RenewRequest, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::{
    lease::TaggedLease,
    protocol::{Down, Renewal, Up},
};
use afd_gateway::test_util::Session;
use std::num::NonZeroU64;

use super::burst::Pickups;
use super::turn::Turn;

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
    /// Stop once `reported` reaches this; `None` runs until the deadline.
    pub(super) target: Option<u64>,
    /// What each lease sends between its grant and its report.
    pub(super) turn: Turn,
    /// Under a burst, the appended steers whose pickup each lease times.
    pub(super) pickups: Option<Arc<Pickups>>,
    /// The monitor every outcome is reported to.
    pub(super) abort: Arc<Abort>,
    pub(super) done: tokio_util::sync::CancellationToken,
}

impl Shared {
    /// Whether this window should keep processing notifications.
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
    /// Entries in a notified or capacity offer pass.
    pub(super) claims: u64,
    /// Leases issued to this runner.
    pub(super) leased: u64,
    /// How long a successful pushed offer took.
    pub(super) lease_latency: Latency,
    /// How long the report after it took.
    pub(super) report_latency: Latency,
}

impl Tally {
    /// An empty tally.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            claims: 0,
            leased: 0,
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
        self.claims += other.claims;
        self.leased += other.leased;
        self.lease_latency.merge(&other.lease_latency)?;
        self.report_latency.merge(&other.report_latency)
    }
}

/// Drive a capacity pass, then await a real notification after an empty claim.
///
/// # Errors
/// Preserves frame, domain and histogram failures.
pub(super) async fn drive(
    mut session: Session,
    shared: &Shared,
    mut generations: tokio::sync::watch::Receiver<u64>,
) -> Result<(Tally, Session)> {
    let mut tally = Tally::new()?;
    let abort = shared.abort.token();
    while shared.open() {
        generations.borrow_and_update();
        tally.claims += 1;
        let issued = step(&mut session, shared, &mut tally).await?;
        if !issued {
            tokio::select! {
                () = abort.cancelled() => break,
                () = shared.done.cancelled() => break,
                () = tokio::time::sleep_until(shared.deadline.into()) => break,
                changed = generations.changed() => {
                    changed?;
                }
            }
        }
    }
    Ok((tally, session))
}

/// The offer and all following operations use one gateway-owned target cache.
async fn step(session: &mut Session, shared: &Shared, tally: &mut Tally) -> Result<bool> {
    session.ready(1).await?;
    let started = Instant::now();
    let Some(frame) = session.offer().await? else {
        return Ok(false);
    };
    let Down::Offer {
        lease: TaggedLease::Fleet(lease),
    } = native::decode(&frame)?
    else {
        return Err(ErrorKind::StreamAnswer.into());
    };
    tally.leased += 1;
    tally.lease_latency.record(started.elapsed())?;
    if let Some(pickups) = &shared.pickups {
        pickups.leased(&lease.event.event_id).await?;
    }
    let folded_through = run_turn(session, &lease, shared).await?;
    let request = processed(&lease, folded_through);
    let reporting = Instant::now();
    let frame = session
        .request(&Up::Report {
            request_id: NonZeroU64::MIN,
            request: Box::new(Cow::Borrowed(&request)),
        })
        .await?;
    let Down::ReportAck {
        lease_id, answer, ..
    } = native::decode(&frame)?
    else {
        return Err(ErrorKind::StreamAnswer.into());
    };
    if lease_id != lease.lease_id || !native::accepted(answer)?.ok {
        return Err(ErrorKind::StreamAnswer.into());
    }
    tally.report_latency.record(reporting.elapsed())?;
    let reported = shared.reported.fetch_add(1, Ordering::Relaxed) + 1;
    if shared.target.is_some_and(|target| reported >= target) {
        shared.done.cancel();
    }
    shared.abort.record(true);
    Ok(true)
}

/// Renew, publish and fold through the same native dispatch used by the listener.
async fn run_turn(
    session: &mut Session,
    lease: &LeasePayload<'_>,
    shared: &Shared,
) -> Result<Option<i64>> {
    let turn = shared.turn;
    renew(session, lease, turn.renewals).await?;
    for seq in 0..turn.posts {
        session
            .send(&Up::Activity {
                lease_id: Cow::Borrowed(&lease.lease_id),
                request: ActivityRequest {
                    frames: vec![chunk(seq)],
                },
            })
            .await?;
    }
    match turn.hold {
        Some(hold) => fold(session, lease, shared, hold).await,
        None => Ok(None),
    }
}

async fn renew(session: &mut Session, lease: &LeasePayload<'_>, renewals: u64) -> Result<()> {
    for _ in 0..renewals {
        let frame = session
            .request(&Up::RenewBatch {
                request_id: NonZeroU64::MIN,
                leases: vec![Renewal {
                    lease_id: Cow::Borrowed(&lease.lease_id),
                    request: RenewRequest {
                        requests: af_wire::usage::RequestLines::default(),
                    },
                }],
            })
            .await?;
        let Down::Renewed { leases, .. } = native::decode(&frame)? else {
            return Err(ErrorKind::StreamAnswer.into());
        };
        if leases.len() != 1 {
            return Err(ErrorKind::StreamAnswer.into());
        }
        for renewed in leases {
            if renewed.lease_id != lease.lease_id {
                return Err(ErrorKind::StreamAnswer.into());
            }
            native::accepted(renewed.answer)?;
        }
    }
    Ok(())
}

async fn fold(
    session: &mut Session,
    lease: &LeasePayload<'_>,
    shared: &Shared,
    hold: Duration,
) -> Result<Option<i64>> {
    let mut through: i64 = 0;
    let ends = Instant::now() + hold;
    while Instant::now() < ends {
        let frame = session
            .request(&Up::Folds {
                request_id: NonZeroU64::MIN,
                lease_id: Cow::Borrowed(&lease.lease_id),
                request: FoldRequest {
                    fencing_token: lease.fencing_token,
                    after_sequence: through,
                },
            })
            .await?;
        let Down::Folded {
            lease_id, answer, ..
        } = native::decode(&frame)?
        else {
            return Err(ErrorKind::StreamAnswer.into());
        };
        if lease_id != lease.lease_id {
            return Err(ErrorKind::StreamAnswer.into());
        }
        for input in native::accepted(answer)?.inputs {
            through = through.max(input.sequence);
            if let Some(pickups) = &shared.pickups {
                pickups.folded(&input.event_id).await?;
            }
        }
        tokio::time::sleep(MODEL_CALL.min(ends.saturating_duration_since(Instant::now()))).await;
    }
    Ok((through > 0).then_some(through))
}

/// How long a simulated model call takes: a held turn claims new messages
/// before each one, as a runner does before every model call.
const MODEL_CALL: Duration = Duration::from_millis(500);

/// One answer chunk, as the runner's pump forwards a streamed model output.
const fn chunk(seq: u64) -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed(RESPONSE),
        text_kind: StreamTextKind::Answer,
        first_chunk_after_ms: None,
        stream_start: seq == 0,
        stream_contiguous: true,
        stream_seq: seq,
    })
}

/// The report a runner that ran the event and answered sends back.
fn processed<'a>(lease: &'a LeasePayload<'a>, folded_through: Option<i64>) -> ReportRequest<'a> {
    ReportRequest {
        folded_through_sequence: folded_through,
        task: None,
        lease_id: Cow::Borrowed(&lease.lease_id),
        event_id: Cow::Borrowed(&lease.event.event_id),
        fencing_token: lease.fencing_token,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE),
        tokens: 0,
        requests: af_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 0,
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event.event_id),
            last_response: Cow::Borrowed(RESPONSE),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}
