//! A spend ceiling at each rule garde proves it against, in rule order; and the
//! stage-chunk threshold held to a finite fraction.

use afd_core::error_code;
use garde::Validate as _;
use serde_json::Map;

use super::{
    DAILY_DOLLARS, Dollars, MAX_DAILY_DOLLARS, REASON_ABOVE_CAP, REASON_NOT_FINITE,
    REASON_NOT_POSITIVE,
};
use crate::config::raw;
use crate::error::ErrorKind;

/// The reason an amount is refused for, or `None` when it is a ceiling.
fn refused(amount: f64) -> Option<String> {
    Dollars::parse(DAILY_DOLLARS, amount, MAX_DAILY_DOLLARS)
        .err()
        .map(|error| error.to_string())
}

/// A `TRIGGER.md` declaring `daily` as its daily ceiling.
fn trigger(daily: &str) -> String {
    format!(
        "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: {daily}\n---\n\n"
    )
}

#[test]
fn test_budget_refuses_nan_and_infinity() {
    // Through the document an author writes: YAML can spell both, and either
    // is an invalid configuration, never a ceiling.
    for spelled in [".nan", ".inf", "-.inf"] {
        assert_eq!(
            crate::parse_trigger(&trigger(spelled))
                .err()
                .map(|error| error.code()),
            Some(error_code::AGENTSFLEET_INVALID_CONFIG),
            "budget {spelled}"
        );
    }
    // And through the rule itself, for a config built from anything that can
    // carry a non-finite float: the reason names finiteness, not the cap.
    for amount in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let reason = refused(amount);
        assert!(
            reason
                .as_deref()
                .is_some_and(|reason| reason.contains(REASON_NOT_FINITE)),
            "{amount} answered {reason:?}"
        );
    }
}

#[test]
fn each_rule_answers_its_own_reason_in_rule_order() {
    for (amount, expected) in [
        (0.0, REASON_NOT_POSITIVE),
        (-1.0, REASON_NOT_POSITIVE),
        (MAX_DAILY_DOLLARS + 1.0, REASON_ABOVE_CAP),
    ] {
        let reason = refused(amount);
        assert!(
            reason
                .as_deref()
                .is_some_and(|reason| reason.contains(expected)),
            "{amount} answered {reason:?}, not {expected:?}"
        );
    }
    assert_eq!(refused(MAX_DAILY_DOLLARS), None);
    assert_eq!(refused(f64::MIN_POSITIVE), None);
    assert_eq!(
        crate::parse_trigger(&trigger("1.5"))
            .err()
            .map(|error| error.to_string()),
        None
    );
}

/// A `TRIGGER.md` declaring `threshold` as its stage-chunk threshold.
fn chunked(threshold: &str) -> String {
    format!(
        "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1\n  context:\n    stage_chunk_threshold: {threshold}\n---\n\n"
    )
}

/// A context block holding only `threshold`, as serde would read it.
fn context(threshold: f32) -> raw::Context {
    raw::Context {
        context_cap_tokens: None,
        tool_window: None,
        memory_checkpoint_every: None,
        stage_chunk_threshold: Some(threshold),
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

    // Through the document: `1e39` is past `f32`'s range and arrives as +∞.
    // Each is the bound's refusal, the one the network allow-lists beside it
    // answer, and never a type error.
    for spelled in ["1e39", "-1e39", "1.5"] {
        let refusal = crate::parse_trigger(&chunked(spelled)).err();
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
    let threshold = crate::parse_trigger(&chunked("0.75"))
        .ok()
        .and_then(|parsed| parsed.config().context())
        .map(|budget| budget.stage_chunk_threshold);
    assert_eq!(threshold, Some(0.75));
}
