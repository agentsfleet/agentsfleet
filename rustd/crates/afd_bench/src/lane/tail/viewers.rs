//! One rung of the viewer ladder: N viewers on one fleet, frames of one size.
//!
//! # How a frame's latency is read without touching the frame
//!
//! Parsing a timestamp out of each payload would add an allocation per viewer
//! per frame to the very number being measured. Instead the publisher stamps
//! the instant it published frame `k` into slot `k` of a shared table, and a
//! viewer, which receives frames in publish order, reads slot `k` when its
//! `k`th frame arrives. Order is guaranteed per channel, and a frame lost in
//! between is caught by the delivered count rather than by the latency.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_dragonfly::FleetStreams;
use afd_sse::{Frame, KIND_CATCHING_UP, Live};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::publish::{FRAME_KIND, PROBE_KIND, frame_of, probe_until_ready, publish};
use super::{
    ALLOCATIONS_PER_FRAME, BUSY_US_PER_FRAME, DELIVERED_FRACTION, LADDER_PAYLOAD_BYTES,
    LADDER_VIEWERS, RECEIVE_P95_MS, push,
};
use crate::allocations::Snapshot;
use crate::error::{Error, Result};
use crate::report::{Latency, Report, count, latency, ratio};

/// Frames each rung publishes.
pub(super) const FRAMES: u64 = 100;

/// Nanoseconds per microsecond, for the busy-time series.
const NANOS_PER_MICRO: f64 = 1_000.0;

/// The task role a lost viewer is reported under.
const VIEWER_ROLE: &str = "tail viewer";

/// What every viewer of one rung shares with its publisher.
#[derive(Debug)]
pub(super) struct Shared {
    /// When the rung started, the zero of every stamp below.
    pub(super) epoch: Instant,
    /// Nanoseconds after `epoch` at which frame `k` was published.
    pub(super) published_at: Vec<AtomicU64>,
    /// Frames delivered to any viewer so far.
    pub(super) delivered: AtomicU64,
    /// Viewers that have seen a probe, and so are known to be subscribed.
    pub(super) ready: AtomicU64,
    /// Woken on every delivery, so the publisher can wait without spinning.
    pub(super) progress: Notify,
}

/// What one viewer saw.
#[derive(Debug)]
struct Seen {
    frames: u64,
    lagged: u64,
    latency: Latency,
}

/// One rung's result.
#[derive(Debug)]
pub(super) struct Rung {
    viewers: u64,
    bytes: usize,
    /// Frames × viewers that arrived.
    pub(super) delivered: u64,
    /// Lag notices any viewer received.
    pub(super) lagged: u64,
    allocations: u64,
    busy: Duration,
    latency: Latency,
}

impl Rung {
    /// Frames × viewers the rung published for.
    pub(super) const fn expected(&self) -> u64 {
        FRAMES * self.viewers
    }

    /// Append this rung to the viewer series.
    pub(super) fn record(&self, report: &mut Report, counting: bool) {
        push(report, LADDER_VIEWERS, count(self.viewers));
        push(
            report,
            LADDER_PAYLOAD_BYTES,
            count(u64::try_from(self.bytes).unwrap_or(u64::MAX)),
        );
        push(
            report,
            DELIVERED_FRACTION,
            ratio(self.delivered, self.expected()),
        );
        // Written only when the counter is installed: an uninstalled one
        // never moves, and its zero would be a measurement nobody took.
        if counting {
            push(
                report,
                ALLOCATIONS_PER_FRAME,
                ratio(self.allocations, self.delivered),
            );
        }
        let busy_ns = u64::try_from(self.busy.as_nanos()).unwrap_or(u64::MAX);
        push(
            report,
            BUSY_US_PER_FRAME,
            ratio(busy_ns, self.delivered) / NANOS_PER_MICRO,
        );
        if !self.latency.is_empty() {
            push(
                report,
                RECEIVE_P95_MS,
                self.latency.quantile_ms(latency::P95),
            );
        }
    }
}

/// Run one rung: subscribe `viewers`, publish [`FRAMES`] of `bytes` each.
///
/// # Errors
///
/// A publish Dragonfly refused, a viewer task that was lost, or a latency the
/// histogram would not hold.
pub(super) async fn rung(
    live: &Live,
    publisher: &FleetStreams,
    fleet: &str,
    viewers: u64,
    bytes: usize,
) -> Result<Rung> {
    let shared = Arc::new(Shared {
        epoch: Instant::now(),
        published_at: (0..FRAMES).map(|_| AtomicU64::new(0)).collect(),
        delivered: AtomicU64::new(0),
        ready: AtomicU64::new(0),
        progress: Notify::new(),
    });
    let stop = CancellationToken::new();
    let mut tasks = Vec::new();
    for _viewer in 0..viewers {
        let (tail, shared, stop) = (live.tail_of(fleet), Arc::clone(&shared), stop.clone());
        tasks.push(tokio::spawn(view(tail, shared, stop)));
    }
    probe_until_ready(publisher, fleet, &shared, viewers).await?;
    let payload = frame_of(bytes);
    let (allocated, busy_before) = (Snapshot::now(), busy_time());
    let sent = publish(publisher, fleet, &payload, &shared, viewers).await;
    let (allocations, busy) = (
        Snapshot::now().allocations_since(allocated),
        busy_time().saturating_sub(busy_before),
    );
    // Only stragglers are still running: a viewer that got every frame has
    // already returned. Stopping them keeps what they did receive.
    stop.cancel();
    let mut rung = Rung {
        viewers,
        bytes,
        delivered: 0,
        lagged: 0,
        allocations,
        busy,
        latency: Latency::new()?,
    };
    for task in tasks {
        let seen = task
            .await
            .map_err(|_lost| Error::TaskLost { role: VIEWER_ROLE })??;
        rung.delivered += seen.frames;
        rung.lagged += seen.lagged;
        rung.latency.merge(&seen.latency)?;
    }
    sent.map(|()| rung)
}

/// One viewer: pull frames until every published one has arrived, or until
/// the rung stops waiting for it.
async fn view(
    mut tail: BoxStream<'static, Frame>,
    shared: Arc<Shared>,
    stop: CancellationToken,
) -> Result<Seen> {
    let mut seen = Seen {
        frames: 0,
        lagged: 0,
        latency: Latency::new()?,
    };
    let mut subscribed = false;
    while seen.frames < FRAMES {
        let frame = tokio::select! {
            biased;
            () = stop.cancelled() => break,
            frame = tail.next() => frame,
        };
        let Some(frame) = frame else {
            break;
        };
        match frame.kind.as_ref() {
            FRAME_KIND => seen.arrived(&shared)?,
            PROBE_KIND if !subscribed => {
                subscribed = true;
                shared.ready.fetch_add(1, Ordering::Relaxed);
            }
            KIND_CATCHING_UP => seen.lagged += 1,
            _ => {}
        }
    }
    Ok(seen)
}

impl Seen {
    /// Count the next measured frame and how long it took to arrive.
    fn arrived(&mut self, shared: &Shared) -> Result<()> {
        let slot = usize::try_from(self.frames).unwrap_or(usize::MAX);
        let stamp = shared
            .published_at
            .get(slot)
            .map_or(0, |at| at.load(Ordering::Acquire));
        let received = u64::try_from(shared.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.latency
            .record(Duration::from_nanos(received.saturating_sub(stamp)))?;
        self.frames += 1;
        shared.delivered.fetch_add(1, Ordering::Relaxed);
        shared.progress.notify_one();
        Ok(())
    }
}

/// Time the runtime's workers have spent busy, summed across workers.
///
/// A worker is busy while it polls a task, so this is the CPU the pump, the
/// viewers and the publisher took between two reads, less whatever the
/// operating system preempted — which is why it is labelled busy time rather
/// than CPU time.
fn busy_time() -> Duration {
    let metrics = tokio::runtime::Handle::current().metrics();
    (0..metrics.num_workers())
        .map(|worker| metrics.worker_total_busy_duration(worker))
        .sum()
}
