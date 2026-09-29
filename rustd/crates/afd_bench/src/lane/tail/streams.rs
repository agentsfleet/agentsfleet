//! One rung of the stream ladder: N open streams, one fleet each, and the
//! heap each one holds.
//!
//! # Held open through a frame, then weighed
//!
//! A stream that has never delivered anything has not yet paid for its
//! subscription's buffer, its frame, or the hub's channel entry on the server
//! round trip. So every stream is held until it has received one probe, and
//! the heap is read while all of them are open and idle — the resting cost of
//! a viewer tab left on a quiet fleet, which is what a replica accumulates.
//!
//! Each stream is the tenant route's own per-fleet stream: an admitted
//! ceiling slot and `Live::tail_of`, pulled by a task the way the SSE body is.
//!
//! # Then timed through one frame each
//!
//! A ceiling is only as good as its latency at that many streams, so once the
//! heap is read the rung publishes one measured frame to every stream's fleet,
//! stamping each publish, and every stream records how long its own took to
//! arrive. The p95 over the streams is the rung's publish-to-receive figure;
//! a stream that never hears its frame is counted, not timed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use afd_dragonfly::FleetStreams;
use afd_sse::{Frame, Live, Slot};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio_util::sync::CancellationToken;

use super::publish::{FRAME_KIND, PROBE, PROBE_INTERVAL, PROBE_KIND, RUNG_DEADLINE, frame_of};
use super::{HEAP_BYTES_PER_STREAM, LADDER_STREAMS, STREAM_RECEIVE_P95_MS, STREAMS_LIVE, push};
use crate::allocations::Snapshot;
use crate::error::{Error, Result};
use crate::fixture::RunPrefix;
use crate::report::{Latency, Report, count, ratio};

/// Bytes in the frame each stream is timed through: a short chunk.
const TIMED_FRAME_BYTES: usize = 200;

/// A stamp or an arrival not yet written.
const UNSET: u64 = 0;

/// The task role a lost stream is reported under.
const STREAM_ROLE: &str = "tail stream";

/// One rung's result.
#[derive(Debug)]
pub(super) struct Rung {
    opened: u64,
    /// Streams that received their probe.
    pub(super) live: u64,
    /// Heap gained between opening the first stream and all of them idling.
    heap_bytes: u64,
    /// Streams whose timed frame arrived.
    pub(super) timed: u64,
    /// Publish-to-receive over the streams whose timed frame arrived.
    latency: Latency,
}

impl Rung {
    /// Append this rung to the stream series.
    pub(super) fn record(&self, report: &mut Report, counting: bool) {
        push(report, LADDER_STREAMS, count(self.opened));
        push(report, STREAMS_LIVE, count(self.live));
        // As in the viewer ladder: no counter, no figure.
        if counting {
            push(
                report,
                HEAP_BYTES_PER_STREAM,
                ratio(self.heap_bytes, self.opened),
            );
        }
        // No frame timed, no figure: a p95 of nothing is not a zero.
        if self.timed > 0 {
            push(
                report,
                STREAM_RECEIVE_P95_MS,
                self.latency.quantile_ms(0.95),
            );
        }
    }
}

/// What every stream of one rung shares with the rung.
#[derive(Debug)]
struct Watch {
    /// Streams that have heard a probe, and so are known to be subscribed.
    heard: Vec<AtomicBool>,
    /// The zero of every stamp below.
    epoch: Instant,
    /// Nanoseconds after `epoch` at which stream `i`'s timed frame was
    /// published, plus one so that zero stays "not yet".
    stamped: Vec<AtomicU64>,
    /// Nanoseconds that frame took to arrive at stream `i`, plus one.
    arrived: Vec<AtomicU64>,
}

impl Watch {
    fn new(streams: usize) -> Self {
        let unset = || (0..streams).map(|_| AtomicU64::new(UNSET)).collect();
        Self {
            heard: (0..streams).map(|_| AtomicBool::new(false)).collect(),
            epoch: Instant::now(),
            stamped: unset(),
            arrived: unset(),
        }
    }

    /// Nanoseconds since the epoch, plus one.
    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .saturating_add(1)
    }

    /// Stream `index` heard its timed frame now.
    fn arrive(&self, index: usize) {
        let stamp = self
            .stamped
            .get(index)
            .map_or(UNSET, |it| it.load(Ordering::Acquire));
        if let (false, Some(slot)) = (stamp == UNSET, self.arrived.get(index)) {
            slot.store(
                self.now().saturating_sub(stamp).saturating_add(1),
                Ordering::Release,
            );
        }
    }

    fn count(flags: impl Iterator<Item = bool>) -> u64 {
        u64::try_from(flags.filter(|it| *it).count()).unwrap_or(u64::MAX)
    }
}

/// Open `opened` streams, one fleet each, and weigh them once each is live.
///
/// # Errors
///
/// A publish Dragonfly refused, or a stream task that was lost.
pub(super) async fn rung(
    live: &Live,
    publisher: &FleetStreams,
    prefix: &RunPrefix,
    opened: u64,
) -> Result<Rung> {
    let stop = CancellationToken::new();
    let fleets: Vec<String> = (0..opened)
        .map(|index| prefix.name(&format!("tail-stream-{opened}-{index}")))
        .collect();
    let watch = Arc::new(Watch::new(fleets.len()));
    let mut tasks = Vec::with_capacity(fleets.len());
    // Taken after the rung's own bookkeeping is allocated, so what is weighed
    // is the streams and nothing the lane needed to hold them.
    let before = Snapshot::now();
    for (index, fleet) in fleets.iter().enumerate() {
        // A stream the ceiling refuses is never heard, and the shortfall is
        // what the rung reports — the ceiling is sized for the widest rung, so
        // a refusal would mean an earlier rung's streams had not closed.
        let Some(slot) = live.admit() else {
            continue;
        };
        let tail = live.tail_of(fleet);
        let (watch, stop) = (Arc::clone(&watch), stop.clone());
        tasks.push(tokio::spawn(hold(tail, slot, watch, index, stop)));
    }
    probe_until_heard(publisher, &fleets, &watch.heard).await?;
    let heap_bytes = Snapshot::now().bytes_gained_since(before);
    time_every_stream(publisher, &fleets, &watch).await?;
    stop.cancel();
    for task in tasks {
        task.await
            .map_err(|_lost| Error::TaskLost { role: STREAM_ROLE })?;
    }
    Ok(Rung {
        opened,
        live: Watch::count(watch.heard.iter().map(|it| it.load(Ordering::Relaxed))),
        heap_bytes,
        timed: Watch::count(
            watch
                .arrived
                .iter()
                .map(|it| it.load(Ordering::Acquire) != UNSET),
        ),
        latency: latency_of(&watch)?,
    })
}

/// Publish one timed frame to every fleet, stamping each, then wait for every
/// stream to hear its own or the deadline.
async fn time_every_stream(
    publisher: &FleetStreams,
    fleets: &[String],
    watch: &Watch,
) -> Result<()> {
    let frame = frame_of(TIMED_FRAME_BYTES);
    for (fleet, stamp) in fleets.iter().zip(&watch.stamped) {
        stamp.store(watch.now(), Ordering::Release);
        publisher.publish_tail(fleet, &frame).await?;
    }
    let deadline = Instant::now() + RUNG_DEADLINE;
    while watch
        .arrived
        .iter()
        .any(|it| it.load(Ordering::Acquire) == UNSET)
        && Instant::now() < deadline
    {
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
    Ok(())
}

/// The arrivals as a distribution.
fn latency_of(watch: &Watch) -> Result<Latency> {
    let mut latency = Latency::new()?;
    for arrived in &watch.arrived {
        let nanos = arrived.load(Ordering::Acquire);
        if nanos != UNSET {
            latency.record(Duration::from_nanos(nanos.saturating_sub(1)))?;
        }
    }
    Ok(latency)
}

/// Hold one stream open, pulling frames, until the rung is done with it.
async fn hold(
    mut tail: BoxStream<'static, Frame>,
    _slot: Slot,
    watch: Arc<Watch>,
    index: usize,
    stop: CancellationToken,
) {
    loop {
        let frame = tokio::select! {
            biased;
            () = stop.cancelled() => return,
            frame = tail.next() => frame,
        };
        match frame {
            Some(frame) if frame.kind == PROBE_KIND => {
                if let Some(flag) = watch.heard.get(index) {
                    flag.store(true, Ordering::Relaxed);
                }
            }
            Some(frame) if frame.kind == FRAME_KIND => watch.arrive(index),
            Some(_other) => {}
            None => return,
        }
    }
}

/// Probe every stream's fleet until each has heard one, or the deadline.
///
/// Only the fleets still silent are probed again, so a rung of four thousand
/// streams repeats a handful of publishes rather than four thousand.
async fn probe_until_heard(
    publisher: &FleetStreams,
    fleets: &[String],
    heard: &[AtomicBool],
) -> Result<()> {
    let deadline = Instant::now() + RUNG_DEADLINE;
    loop {
        let mut silent = 0_usize;
        for (fleet, flag) in fleets.iter().zip(heard) {
            if !flag.load(Ordering::Relaxed) {
                silent += 1;
                publisher.publish_tail(fleet, PROBE).await?;
            }
        }
        if silent == 0 || Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
}
