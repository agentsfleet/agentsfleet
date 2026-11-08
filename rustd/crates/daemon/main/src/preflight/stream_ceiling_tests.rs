//! The default stream ceiling, held to the rung the tail lane measured.
//!
//! `SSE_MAX_STREAMS_DEFAULT`'s doc cites a `make bench-tail PROFILE=rig` run.
//! The number and the run are two files, so a default raised without a run
//! behind it, or a re-run whose rung missed a bound, would leave the doc citing
//! evidence that no longer holds. `build.rs` carries the committed baseline in;
//! this grades it.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; an unmet precondition should stop it"
)]

use super::knobs::SSE_MAX_STREAMS_DEFAULT;

/// The series and counts `build.rs` read out of `bench/baselines/tail.rig.json`.
mod baseline {
    include!(concat!(env!("OUT_DIR"), "/tail_baseline.rs"));
}

use baseline::{
    FRAMES_UNDELIVERED, HEAP_BYTES_PER_STREAM, LADDER_STREAMS, STREAM_RECEIVE_P95_MS, STREAMS_LIVE,
    STREAMS_UNREACHED,
};

/// Publish-to-receive p95 a rung must stay under, in milliseconds.
const P95_BOUND_MS: f64 = 250.0;

/// Heap the whole rung must stay under: 256 MiB.
const HEAP_BOUND_BYTES: f64 = 256.0 * 1024.0 * 1024.0;

#[test]
fn the_default_stream_ceiling_is_a_measured_rung_inside_its_bounds() {
    assert!(
        !LADDER_STREAMS.is_empty(),
        "build.rs read no stream ladder from bench/baselines/tail.rig.json; the read is broken, not the baseline"
    );
    let default = f64::from(
        u32::try_from(SSE_MAX_STREAMS_DEFAULT).expect("the default stream ceiling fits a u32"),
    );
    assert_eq!(
        STREAM_RECEIVE_P95_MS.len(),
        LADDER_STREAMS.len(),
        "every rung carries its p95"
    );
    let at = LADDER_STREAMS
        .iter()
        .position(|rung| rung.total_cmp(&default).is_eq())
        .expect("SSE_MAX_STREAMS_DEFAULT is a rung the tail lane measured");

    let live = STREAMS_LIVE
        .get(at)
        .expect("the rung records its live streams");
    let heap = HEAP_BYTES_PER_STREAM
        .get(at)
        .expect("the rung records its heap per stream");
    let p95 = STREAM_RECEIVE_P95_MS.get(at).expect("checked above");
    assert!(
        live.total_cmp(&default).is_eq(),
        "the {default}-stream rung reached {live} streams, not every one"
    );
    for (count, what) in [
        (FRAMES_UNDELIVERED, "frames_undelivered"),
        (STREAMS_UNREACHED, "streams_unreached"),
    ] {
        assert!(
            count.is_some_and(|lost| lost <= 0.0),
            "the baseline's {what} is {count:?}; it lost work"
        );
    }
    assert!(
        *p95 < P95_BOUND_MS,
        "the {default}-stream rung's p95 {p95} ms misses its {P95_BOUND_MS} ms bound"
    );
    assert!(
        heap * default < HEAP_BOUND_BYTES,
        "the {default}-stream rung's heap {} bytes misses its {HEAP_BOUND_BYTES} byte bound",
        heap * default
    );
}
