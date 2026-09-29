//! What the live tail costs: per delivered frame as viewers pile onto one
//! fleet, and per open stream as a replica holds more of them.
//!
//! # Two ladders, one hub
//!
//! ```text
//!   publisher ──SPUBLISH──► Dragonfly ──push──► hub (one per process)
//!                                              │  broadcast per channel
//!                                              ├──► viewer 1 ─► afd_sse tail ─► frame
//!                                              ├──► viewer 2 ─► …
//!                                              └──► viewer N
//! ```
//!
//! The VIEWER ladder puts 1, 64, 256 and 1024 viewers on one fleet's channel
//! and publishes frames of 200 B, 4 KiB and 64 KiB through the product's own
//! `SPUBLISH` path. Each viewer is the per-fleet SSE stream the tenant route
//! serves (`afd_sse::Live::tail_of`), so what a frame costs includes the
//! per-viewer copy and the frame built from it — the costs a shared payload
//! would remove. It reports allocations, runtime busy time and publish-to-
//! receive latency per delivered frame.
//!
//! The STREAM ladder opens 64, 256, 1024 and 4096 such streams, each on its
//! own fleet, holds each through one delivered frame, and reports the heap
//! each one keeps and the p95 of one frame published to every one of them.
//! Those are the numbers `SSE_MAX_STREAMS` should be sized from.
//!
//! # Every frame must arrive, so the publisher waits for the slowest viewer
//!
//! The hub buffers a bounded number of messages per channel and tells a reader
//! that falls further behind that it lagged. A ladder that outran its slowest
//! viewer would measure the lag path rather than delivery, so the publisher
//! keeps no more than [`publish::WINDOW`] frames ahead of the slowest one. A
//! frame that still does not arrive is counted and reported, never assumed.
//!
//! # A node's loss is not measured here
//!
//! Killing one process of the shared compose cluster is not a stable bench;
//! the live tail's behaviour on a node loss is proven by an integration test.

mod publish;
mod streams;
mod viewers;

use afd_dragonfly::FleetStreams;
use afd_sse::{Ceiling, Live};

use crate::allocations;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::{Parameter, Profile};
use crate::report::{Fixture, Lane, Provenance, Report, count};

/// Viewers on one fleet, rung by rung.
pub const VIEWER_LADDER: [u64; 4] = [1, 64, 256, 1024];

/// Bytes per published frame, rung by rung: a short chunk, a tool result, a
/// long answer's final frame.
pub const PAYLOAD_LADDER: [usize; 3] = [200, 4 * BYTES_PER_KIB, 64 * BYTES_PER_KIB];

/// Concurrent streams, rung by rung, one fleet each.
pub const STREAM_LADDER: [u64; 4] = [64, 256, 1024, 4096];

/// Bytes in a kibibyte.
const BYTES_PER_KIB: usize = 1_024;

/// Series key: the viewer count of each viewer rung.
const LADDER_VIEWERS: &str = "ladder_viewers";

/// Series key: the payload size of each viewer rung.
const LADDER_PAYLOAD_BYTES: &str = "ladder_payload_bytes";

/// Series key: allocations per delivered frame.
const ALLOCATIONS_PER_FRAME: &str = "allocations_per_delivered_frame";

/// Series key: runtime busy time per delivered frame, in microseconds.
const BUSY_US_PER_FRAME: &str = "busy_us_per_delivered_frame";

/// Series key: publish-to-receive latency at the 95th percentile.
const RECEIVE_P95_MS: &str = "receive_p95_ms";

/// Series key: the fraction of frames × viewers that arrived.
const DELIVERED_FRACTION: &str = "delivered_fraction";

/// Series key: the stream count of each stream rung.
const LADDER_STREAMS: &str = "ladder_streams";

/// Series key: streams that received their frame, per stream rung.
const STREAMS_LIVE: &str = "streams_live";

/// Series key: heap each open stream holds, in bytes.
const HEAP_BYTES_PER_STREAM: &str = "heap_bytes_per_stream";

/// Series key: publish-to-receive p95 of one frame per stream, per stream rung.
const STREAM_RECEIVE_P95_MS: &str = "stream_receive_p95_ms";

/// Measurement key: frames × viewers that never arrived, over every rung,
/// and timed frames a live stream never heard.
pub const FRAMES_UNDELIVERED: &str = "frames_undelivered";

/// Measurement key: lag notices any viewer received, over every rung.
pub const LAG_NOTICES: &str = "lag_notices";

/// Measurement key: streams that never received their frame, over every rung.
pub const STREAMS_UNREACHED: &str = "streams_unreached";

/// Run both ladders and return the report.
///
/// # Errors
///
/// A cap refusal before anything opens, a hub that will not start, a publish
/// Dragonfly refused, or a lost viewer task. A frame that did not arrive is a
/// measurement, not an error.
pub async fn run(
    profile: Profile,
    provenance: Provenance,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    let widest = STREAM_LADDER
        .iter()
        .chain(&VIEWER_LADDER)
        .max()
        .copied()
        .unwrap_or(0);
    profile.check(Parameter::Concurrency, widest)?;
    let hub = stores.hub().await?;
    let live = Live::new(
        hub.clone(),
        Ceiling::new(usize::try_from(widest).unwrap_or(usize::MAX)),
    );
    let publisher = FleetStreams::new(stores.queue.clone());
    let counting = allocations::installed();
    let mut report = Report::new(Lane::Tail, profile, provenance);
    report.created = true;
    let mut missing = Totals::default();
    for viewers in VIEWER_LADDER {
        for bytes in PAYLOAD_LADDER {
            let fleet = prefix.name(&format!("tail-{viewers}-{bytes}"));
            let rung = viewers::rung(&live, &publisher, &fleet, viewers, bytes).await?;
            missing.frames += rung.expected().saturating_sub(rung.delivered);
            missing.lags += rung.lagged;
            rung.record(&mut report, counting);
        }
    }
    for opened in STREAM_LADDER {
        let rung = streams::rung(&live, &publisher, prefix, opened).await?;
        missing.streams += opened.saturating_sub(rung.live);
        missing.frames += rung.live.saturating_sub(rung.timed);
        rung.record(&mut report, counting);
    }
    hub.shutdown();
    report.measurement(FRAMES_UNDELIVERED, count(missing.frames));
    report.measurement(LAG_NOTICES, count(missing.lags));
    report.measurement(STREAMS_UNREACHED, count(missing.streams));
    report.fixture = Fixture::of(prefix, FixtureLedger::new());
    Ok(report)
}

/// What went missing across every rung, for the three headline numbers.
#[derive(Debug, Default)]
struct Totals {
    frames: u64,
    lags: u64,
    streams: u64,
}

/// The viewer rungs' series, in the order a summary line prints them.
const VIEWER_COLUMNS: [&str; 6] = [
    LADDER_VIEWERS,
    LADDER_PAYLOAD_BYTES,
    DELIVERED_FRACTION,
    ALLOCATIONS_PER_FRAME,
    BUSY_US_PER_FRAME,
    RECEIVE_P95_MS,
];

/// The stream rungs' series, in the order a summary line prints them.
const STREAM_COLUMNS: [&str; 4] = [
    LADDER_STREAMS,
    STREAMS_LIVE,
    HEAP_BYTES_PER_STREAM,
    STREAM_RECEIVE_P95_MS,
];

/// The headline totals, printed last.
const TOTALS: [&str; 3] = [FRAMES_UNDELIVERED, LAG_NOTICES, STREAMS_UNREACHED];

/// Both ladders as `key=value` lines, one per rung, then the totals.
///
/// Printed rather than left only in the file because the ladder is read by a
/// person deciding a stream ceiling, and twelve rungs read better as twelve
/// lines than as six parallel arrays. A series the run did not write — the
/// allocation figures without the counting allocator — is left out.
#[must_use]
pub fn summary(report: &Report) -> String {
    let mut lines = rows(report, &VIEWER_COLUMNS);
    lines.extend(rows(report, &STREAM_COLUMNS));
    lines.push(
        TOTALS
            .iter()
            .filter_map(|key| report.measurements.get(*key).map(|value| pair(key, value)))
            .collect::<Vec<_>>()
            .join(" "),
    );
    lines.join("\n")
}

/// One `key=value` field of a summary line.
fn pair(key: &str, value: impl std::fmt::Display) -> String {
    format!("{key}={value}")
}

/// One line per rung for the series named by `columns`.
fn rows(report: &Report, columns: &[&str]) -> Vec<String> {
    let rungs = columns
        .first()
        .and_then(|key| report.series.get(*key))
        .map_or(0, Vec::len);
    (0..rungs)
        .map(|rung| {
            columns
                .iter()
                .filter_map(|key| {
                    let value = report.series.get(*key)?.get(rung)?;
                    Some(pair(key, value))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Append one value to a named series.
pub(super) fn push(report: &mut Report, key: &str, value: f64) {
    report.series.entry(key.to_owned()).or_default().push(value);
}

#[cfg(test)]
mod tests;
