//! A spend ceiling at each rule garde proves it against, in rule order; and the
//! stage-chunk threshold held to a finite fraction.

use afd_core::error_code;
use afd_core::money::NANOS_PER_USD;
use garde::Validate as _;
use serde_json::{Map, json};

use crate::config::raw;
use crate::error::ErrorKind;

/// A configuration declaring `threshold` as its stage-chunk threshold.
fn chunked(threshold: f64) -> String {
    json!({"name": "probe", "x-agentsfleet": {
        "triggers": [], "tools": [],
        "budget": {"daily_nanos": NANOS_PER_USD.to_string()},
        "context": {"stage_chunk_threshold": threshold}
    }})
    .to_string()
}

/// A context block holding only `threshold`, as serde would read it.
fn context(threshold: f32) -> raw::Context {
    raw::Context {
        context_cap_tokens: None,
        tool_window: None,
        memory_checkpoint_every: None,
        stage_chunk_threshold: Some(f64::from(threshold)),
        extra: Map::new(),
    }
}

#[test]
fn test_stage_chunk_threshold_is_a_finite_fraction() {
    // The declared rule: NaN and both infinities fail `finite` even where the
    // range would let NaN through, and past one is not a fraction.
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.5, -0.25] {
        assert!(context(value).validate().is_err(), "{value} was admitted");
    }
    for value in [0.0, 0.75, 1.0] {
        assert!(context(value).validate().is_ok(), "{value} was refused");
    }

    // Through the document: `1e39` is parsed at JSON precision before bounds.
    // Each is the bound's refusal, the one the network allow-lists beside it
    // answer, and never a type error.
    for spelled in [1e39, -1e39, 1.5] {
        let refusal = crate::FleetConfig::authored(&chunked(spelled)).err();
        assert!(
            refusal
                .as_ref()
                .is_some_and(|error| matches!(error.kind(), ErrorKind::OutOfBounds { .. })),
            "{spelled} answered {refusal:?}"
        );
        assert_eq!(
            refusal.map(|error| error.code()),
            Some(error_code::AGENTSFLEET_INVALID_CONFIG),
            "{spelled}"
        );
    }
    let threshold = crate::FleetConfig::authored(&chunked(0.75))
        .ok()
        .and_then(|parsed| parsed.context())
        .map(|budget| budget.stage_chunk_threshold);
    assert_eq!(threshold, Some(0.75));
}
