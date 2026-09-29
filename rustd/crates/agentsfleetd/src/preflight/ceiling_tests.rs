//! The stream ceiling's default, held against the measurement that set it.
//!
//! `SSE_MAX_STREAMS_DEFAULT` cites the tail lane's stream ladder. This reads
//! the committed baseline that ladder wrote, so the default cannot move to a
//! rung the baseline does not support, and a baseline rewritten without the
//! per-rung latency cannot pass for one that has it.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; an unmet precondition should stop it"
)]

use std::path::PathBuf;

use serde_json::Value;

use super::knobs::SSE_MAX_STREAMS_DEFAULT;

/// The committed tail-lane baseline, from this crate's manifest directory.
const BASELINE: &str = "../../../bench/baselines/tail.rig.json";

/// A rung's publish-to-receive p95 must stay under this, in milliseconds.
const P95_BOUND_MS: f64 = 250.0;

/// A rung's streams together must hold less heap than this, in bytes.
const HEAP_BOUND_BYTES: f64 = 256.0 * 1024.0 * 1024.0;

/// Every frame the lane published must have arrived, over every rung.
const UNDELIVERED_BOUND: f64 = 0.0;

/// One named series of the baseline.
fn series(baseline: &Value, key: &str) -> Vec<f64> {
    baseline
        .pointer(&format!("/series/{key}"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the baseline carries no `{key}` series"))
        .iter()
        .map(|value| value.as_f64().expect("a series holds numbers"))
        .collect()
}

/// One named measurement of the baseline.
fn measurement(baseline: &Value, key: &str) -> f64 {
    baseline
        .pointer(&format!("/measurements/{key}"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("the baseline carries no `{key}` measurement"))
}

fn baseline() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BASELINE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} is unreadable: {failure}", path.display()));
    serde_json::from_str(&text).expect("the baseline is JSON")
}

/// Dimension 5.6: the default is a rung the committed baseline measured, and
/// that rung met every bound — all frames delivered, p95 and total heap under
/// their caps — with every stream rung carrying its p95.
#[test]
fn bench_stream_ceiling_ladder() {
    let baseline = baseline();
    let rungs = series(&baseline, "ladder_streams");
    let live = series(&baseline, "streams_live");
    let heap = series(&baseline, "heap_bytes_per_stream");
    let p95 = series(&baseline, "stream_receive_p95_ms");
    assert_eq!(
        p95.len(),
        rungs.len(),
        "every stream rung carries its publish-to-receive p95"
    );

    let default = f64::from(u32::try_from(SSE_MAX_STREAMS_DEFAULT).expect("a small ceiling"));
    let rung = rungs
        .iter()
        .position(|streams| (*streams - default).abs() < f64::EPSILON)
        .unwrap_or_else(|| panic!("no measured rung of {default} streams: {rungs:?}"));
    let at = |values: &[f64]| *values.get(rung).expect("one value per rung");

    assert!(
        (at(&live) - default).abs() < f64::EPSILON,
        "every stream at the rung was reached: {} of {default}",
        at(&live)
    );
    assert!(
        measurement(&baseline, "frames_undelivered") <= UNDELIVERED_BOUND
            && measurement(&baseline, "streams_unreached") <= UNDELIVERED_BOUND,
        "every frame was delivered"
    );
    assert!(
        at(&p95) < P95_BOUND_MS,
        "p95 {} ms at {default} streams",
        at(&p95)
    );
    assert!(
        at(&heap) * default < HEAP_BOUND_BYTES,
        "{} bytes at {default} streams",
        at(&heap) * default
    );
}
