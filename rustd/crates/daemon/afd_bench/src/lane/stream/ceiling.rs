//! The daemon's default stream ceiling, held to the rungs this lane measured.
//!
//! `afd_sse::MAX_STREAMS_DEFAULT` cites this lane as its evidence.
//! The check runs here, where the measurement happens, so a default raised past
//! what the stream ladder sustains fails the run that would have backed it.

use afd_sse::MAX_STREAMS_DEFAULT;

use super::{HEAP_BYTES_PER_STREAM, LADDER_STREAMS, STREAM_RECEIVE_P95_MS, STREAMS_LIVE};
use crate::error::{ErrorKind, Result};
use crate::report::Report;

/// Publish-to-receive p95 a rung must stay under, in milliseconds.
const P95_BOUND_MS: f64 = 250.0;

/// Heap a whole rung must stay under: 256 MiB.
const HEAP_BOUND_BYTES: f64 = 256.0 * 1024.0 * 1024.0;

/// The widest rung inside its bounds: every stream reached, p95 under
/// [`P95_BOUND_MS`], the rung's heap under [`HEAP_BOUND_BYTES`].
///
/// Zero when no rung is, or when a series skipped a rung: a rung with no
/// timed frame writes no p95, and grading the next rung's p95 against it
/// would pass a rung on another rung's figure.
#[must_use]
pub(super) fn widest_bounded(report: &Report) -> f64 {
    let series = |key: &str| report.series.get(key).map_or(&[][..], Vec::as_slice);
    let ladder = series(LADDER_STREAMS);
    let (live, heap, p95) = (
        series(STREAMS_LIVE),
        series(HEAP_BYTES_PER_STREAM),
        series(STREAM_RECEIVE_P95_MS),
    );
    if [live, heap, p95]
        .iter()
        .any(|each| each.len() != ladder.len())
    {
        return 0.0;
    }
    ladder
        .iter()
        .zip(live)
        .zip(heap)
        .zip(p95)
        .filter(|&(((opened, reached), per_stream), ms)| {
            reached.total_cmp(opened).is_eq()
                && *ms < P95_BOUND_MS
                && per_stream * opened < HEAP_BOUND_BYTES
        })
        .map(|(((opened, _), _), _)| *opened)
        .fold(0.0, f64::max)
}

/// Refuse a run whose rungs no longer back the daemon's default ceiling.
///
/// # Errors
///
/// `CeilingUnbacked` naming the default and the widest rung inside bounds.
pub fn backs_default(report: &Report) -> Result<()> {
    let default = f64::from(u32::try_from(MAX_STREAMS_DEFAULT).unwrap_or(u32::MAX));
    let widest = widest_bounded(report);
    if default > widest {
        return Err(ErrorKind::CeilingUnbacked { default, widest }.into());
    }
    Ok(())
}
