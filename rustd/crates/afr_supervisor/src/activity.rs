//! Live-tail frames: emitted by the run, batched, posted best-effort.
//!
//! ```text
//!   run ──emit──► ActivitySink ──channel──► ActivityPump: batch ≤ 64 KiB ──► poster ──► daemon
//!                                               │  at most four batches held,
//!                                               └─ the fifth is dropped and counted
//! ```
//!
//! Emitting never blocks the run. A slow daemon costs the live tail frames,
//! never the run its progress and never the report its result: past
//! [`MAX_BATCHES_HELD`] batches waiting or in flight, the next is dropped,
//! counted and logged.

use std::io;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::activity::{ActivityFrame, ActivityRequest};
use afr_agent::EventSink;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::time::Instant;

use crate::client::ControlPlane;

/// Batches waiting or in flight at once; the next is dropped.
pub(crate) const MAX_BATCHES_HELD: usize = 4;
/// The most frame bytes one batch carries; a single larger frame rides alone.
pub(crate) const MAX_BATCH_BYTES: usize = 64 * 1024;
/// How long a partial batch waits for company before it is sent anyway.
const FLUSH_EVERY: Duration = Duration::from_millis(250);
const EVENT_BATCH_DROPPED: &str = "activity_batch_dropped";
const EVENT_POST_FAILED: &str = "activity_frame_write_failed";

/// The run's side: hands frames to the pump without waiting, and notes when
/// the first answer chunk went by, which is the run's time to first token.
#[derive(Debug)]
pub(crate) struct ActivitySink {
    frames: mpsc::UnboundedSender<ActivityFrame<'static>>,
    started: Instant,
    first_chunk: OnceLock<Duration>,
}

impl ActivitySink {
    /// When the first answer chunk was emitted, measured from the sink's start.
    pub(crate) fn first_chunk(&self) -> Option<Duration> {
        self.first_chunk.get().copied()
    }
}

impl EventSink for ActivitySink {
    fn emit(&self, frame: ActivityFrame<'static>) {
        if matches!(frame, ActivityFrame::FleetResponseChunk(_)) {
            self.first_chunk.get_or_init(|| self.started.elapsed());
        }
        // A closed pump means the lease is ending; a frame then has no reader.
        drop(self.frames.send(frame));
    }
}

/// The posting side: batches frames and posts them, for one lease.
#[derive(Debug)]
pub(crate) struct ActivityPump<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
    frames: mpsc::UnboundedReceiver<ActivityFrame<'static>>,
    held: Arc<Semaphore>,
    /// Batches dropped because [`MAX_BATCHES_HELD`] were already held.
    dropped: u64,
}

/// A sink for the run and the pump that posts what it emits.
pub(crate) fn channel<'a>(
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
) -> (ActivitySink, ActivityPump<'a>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let pump = ActivityPump {
        plane,
        lease_id,
        frames: receiver,
        held: Arc::new(Semaphore::new(MAX_BATCHES_HELD)),
        dropped: 0,
    };
    let sink = ActivitySink {
        frames: sender,
        started: Instant::now(),
        first_chunk: OnceLock::new(),
    };
    (sink, pump)
}

/// One batch, and the permit that counts it as held until it is posted.
type Held = (Vec<ActivityFrame<'static>>, OwnedSemaphorePermit);

impl ActivityPump<'_> {
    /// Batches and posts until the sink is dropped and every held batch is
    /// sent.
    pub(crate) async fn run(&mut self) {
        let (posts, mut queued) = mpsc::unbounded_channel::<Held>();
        let plane = self.plane;
        let lease_id = self.lease_id;
        let poster = async move {
            while let Some((frames, permit)) = queued.recv().await {
                post(plane, lease_id, frames).await;
                drop(permit);
            }
        };
        tokio::join!(self.batch(posts), poster);
    }

    async fn batch(&mut self, posts: mpsc::UnboundedSender<Held>) {
        let mut batch = Vec::new();
        let mut bytes = 0;
        let mut flush = tokio::time::interval_at(Instant::now() + FLUSH_EVERY, FLUSH_EVERY);
        loop {
            tokio::select! {
                // Frames already queued join the batch before a tick sends it.
                biased;
                frame = self.frames.recv() => {
                    let Some(frame) = frame else { break };
                    let size = encoded_len(&frame);
                    if !batch.is_empty() && bytes + size > MAX_BATCH_BYTES {
                        self.hand_off(&posts, std::mem::take(&mut batch));
                        bytes = 0;
                    }
                    if batch.is_empty() {
                        // A quiet spell left the tick's deadline behind; the
                        // new batch gets a full period, not a burst of ticks.
                        flush.reset();
                    }
                    batch.push(frame);
                    bytes += size;
                }
                _ = flush.tick(), if !batch.is_empty() => {
                    self.hand_off(&posts, std::mem::take(&mut batch));
                    bytes = 0;
                }
            }
        }
        if !batch.is_empty() {
            self.hand_off(&posts, batch);
        }
    }

    /// Queues a batch for posting, or drops it when enough are already held.
    fn hand_off(
        &mut self,
        posts: &mpsc::UnboundedSender<Held>,
        frames: Vec<ActivityFrame<'static>>,
    ) {
        match Arc::clone(&self.held).try_acquire_owned() {
            // The poster outlives the batcher inside `run`, so it is listening.
            Ok(permit) => drop(posts.send((frames, permit))),
            Err(_full) => {
                self.dropped += 1;
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let lease_id = self.lease_id.as_str();
                let dropped = self.dropped;
                let event = EVENT_BATCH_DROPPED;
                tracing::warn!(
                    error_code = code,
                    lease_id,
                    dropped,
                    event,
                    "the live tail fell behind; a batch was dropped"
                );
            }
        }
    }
}

/// Posts one batch. Activity is best-effort: a failure is logged, not retried.
async fn post(plane: &ControlPlane, lease_id: &Uuid7, frames: Vec<ActivityFrame<'static>>) {
    if let Err(failure) = plane.activity(lease_id, &ActivityRequest { frames }).await {
        let code = failure.code().as_str();
        let lease_id = lease_id.as_str();
        let event = EVENT_POST_FAILED;
        tracing::warn!(
            error_code = code,
            lease_id,
            event,
            "a live-tail batch was not delivered"
        );
    }
}

/// A frame's encoded size, measured without allocating its encoding.
fn encoded_len(frame: &ActivityFrame<'_>) -> usize {
    let mut counter = Counter(0);
    // Encoding a wire frame cannot fail; were it to, the frame rides alone.
    serde_json::to_writer(&mut counter, frame).map_or(MAX_BATCH_BYTES, |()| counter.0)
}

/// An `io::Write` that keeps only the count of what was written to it.
struct Counter(usize);

impl io::Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "activity/tests.rs"]
mod tests;
