//! Carries the tail lane's committed baseline into the stream-ceiling test.
//!
//! `SSE_MAX_STREAMS_DEFAULT` claims a rung `make bench-tail PROFILE=rig`
//! measured inside its bounds. The claim and the measurement live in two files,
//! so this script reads `bench/baselines/tail.rig.json` and writes the series
//! the claim rests on into `$OUT_DIR/tail_baseline.rs`, which
//! `src/preflight/stream_ceiling_tests.rs` includes and grades.
//!
//! A baseline that is missing or does not parse writes empty series rather
//! than failing the build: the binary does not need it, and the test refuses
//! an empty ladder as a broken read.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::Value;

/// The committed baseline, from this crate's manifest.
const BASELINE: &str = "../../../../bench/baselines/tail.rig.json";

/// The file the test includes.
const LISTING: &str = "tail_baseline.rs";

/// Each emitted series: the constant's name and the baseline's JSON pointer.
const SERIES: [(&str, &str); 4] = [
    ("LADDER_STREAMS", "/series/ladder_streams"),
    ("STREAMS_LIVE", "/series/streams_live"),
    ("HEAP_BYTES_PER_STREAM", "/series/heap_bytes_per_stream"),
    ("STREAM_RECEIVE_P95_MS", "/series/stream_receive_p95_ms"),
];

/// Each emitted whole-run count: the constant's name and its JSON pointer.
const MEASURED: [(&str, &str); 2] = [
    ("FRAMES_UNDELIVERED", "/measurements/frames_undelivered"),
    ("STREAMS_UNREACHED", "/measurements/streams_unreached"),
];

#[expect(
    clippy::disallowed_methods,
    reason = "a build script has no build-dependency on af_core, and OUT_DIR is cargo's, not a knob"
)]
fn out_dir() -> Option<PathBuf> {
    std::env::var_os("OUT_DIR").map(PathBuf::from)
}

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed={BASELINE}");
    println!("cargo:rerun-if-changed=build.rs");

    let Some(out) = out_dir() else {
        return Err(std::io::Error::other("cargo did not set OUT_DIR"));
    };
    let baseline: Value = std::fs::read_to_string(BASELINE)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    let mut listing = String::new();
    for (name, pointer) in SERIES {
        let values: Vec<f64> = baseline
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        let values: Vec<String> = values.into_iter().map(literal).collect();
        let _ = writeln!(
            listing,
            "pub(super) const {name}: &[f64] = &[{}];",
            values.join(", ")
        );
    }
    for (name, pointer) in MEASURED {
        let value = baseline
            .pointer(pointer)
            .and_then(Value::as_f64)
            .map_or_else(
                || "None".to_owned(),
                |value| format!("Some({})", literal(value)),
            );
        let _ = writeln!(listing, "pub(super) const {name}: Option<f64> = {value};");
    }
    std::fs::write(out.join(LISTING), listing)
}

/// An `f64` as a Rust literal, digits grouped in threes so the included file
/// reads as the crate's own source would (`clippy::unreadable_literal`).
fn literal(value: f64) -> String {
    // `{:?}` always writes a float literal: `64.0`, `13521.05078125`, `1e-7`.
    let written = format!("{value:?}");
    let (mantissa, exponent) = written
        .split_once('e')
        .map_or((written.as_str(), None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let (sign, whole) = whole
        .strip_prefix('-')
        .map_or(("", whole), |digits| ("-", digits));
    let mut grouped = String::from(sign);
    for (at, digit) in whole.chars().enumerate() {
        if at > 0 && (whole.len() - at) % 3 == 0 {
            grouped.push('_');
        }
        grouped.push(digit);
    }
    if !fraction.is_empty() {
        grouped.push('.');
        for (at, digit) in fraction.chars().enumerate() {
            if at > 0 && at % 3 == 0 {
                grouped.push('_');
            }
            grouped.push(digit);
        }
    }
    if let Some(exponent) = exponent {
        grouped.push('e');
        grouped.push_str(exponent);
    }
    grouped
}
