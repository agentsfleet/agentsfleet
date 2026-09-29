//! The publishing half of a viewer rung: readiness probes, the frames, and the
//! flow control that keeps the publisher inside the hub's buffer.
//!
//! Split from the viewers at the seam the rung has: a viewer pulls and counts,
//! the publisher stamps and paces, and they meet only in [`Shared`].

use core::time::Duration;
use std::sync::atomic::Ordering;
use std::time::Instant;

use afd_dragonfly::FleetStreams;

use super::viewers::{FRAMES, Shared};
use crate::error::Result;

/// The kind every measured frame carries, so a viewer can tell it from the
/// connection's own `hello` and from the readiness probes.
pub(super) const FRAME_KIND: &str = "bench_frame";

/// How far the publisher may run ahead of the slowest viewer, in frames.
///
/// A quarter of the hub's per-channel buffer: far enough that the pipeline
/// stays full, short enough that no viewer can be lapped into a lag notice.
pub(super) const WINDOW: u64 = 64;

/// The kind a readiness probe carries.
pub(super) const PROBE_KIND: &str = "bench_probe";

/// A probe's whole payload.
pub(super) const PROBE: &str = r#"{"kind":"bench_probe"}"#;

/// How often an unanswered probe is repeated.
pub(super) const PROBE_INTERVAL: Duration = Duration::from_millis(10);

/// The longest a rung may wait for its viewers to subscribe or its frames to
/// arrive before it reports what it has.
pub(super) const RUNG_DEADLINE: Duration = Duration::from_secs(60);

/// Probe the channel until every viewer has seen a probe, or the deadline.
///
/// A viewer's receiver exists from the moment it subscribes, but the hub
/// subscribes server-side asynchronously: a frame published before that lands
/// is delivered to nobody. Probing until every viewer has seen one is the only
/// proof that the next frame reaches all of them.
pub(super) async fn probe_until_ready(
    publisher: &FleetStreams,
    fleet: &str,
    shared: &Shared,
    viewers: u64,
) -> Result<()> {
    let deadline = Instant::now() + RUNG_DEADLINE;
    while shared.ready.load(Ordering::Relaxed) < viewers && Instant::now() < deadline {
        publisher.publish_tail(fleet, PROBE).await?;
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
    Ok(())
}

/// Publish every frame, never more than [`WINDOW`] ahead of the slowest
/// viewer, then wait for the last to arrive.
pub(super) async fn publish(
    publisher: &FleetStreams,
    fleet: &str,
    payload: &str,
    shared: &Shared,
    viewers: u64,
) -> Result<()> {
    let deadline = Instant::now() + RUNG_DEADLINE;
    for (index, slot) in (0_u64..).zip(&shared.published_at) {
        let behind = index.saturating_sub(WINDOW).saturating_mul(viewers);
        wait_for(shared, behind, deadline).await;
        let stamp = u64::try_from(shared.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX);
        slot.store(stamp, Ordering::Release);
        publisher.publish_tail(fleet, payload).await?;
    }
    wait_for(shared, FRAMES.saturating_mul(viewers), deadline).await;
    Ok(())
}

/// Wait until `delivered` reaches `target`, or the deadline passes.
async fn wait_for(shared: &Shared, target: u64, deadline: Instant) {
    while shared.delivered.load(Ordering::Relaxed) < target {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero()
            || tokio::time::timeout(left, shared.progress.notified())
                .await
                .is_err()
        {
            return;
        }
    }
}

/// A measured frame of exactly `bytes` bytes, where that fits its envelope.
pub(super) fn frame_of(bytes: usize) -> String {
    let head = format!(r#"{{"kind":"{FRAME_KIND}","pad":""#);
    let tail = r#""}"#;
    let pad = bytes.saturating_sub(head.len() + tail.len());
    format!("{head}{}{tail}", "x".repeat(pad))
}

#[cfg(test)]
mod tests;
