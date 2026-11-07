//! Each threshold at its cap, past it and at zero, and the order a rule's
//! faults are asked in.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{
    MAX_ANOMALY_REPEATS, MAX_ANOMALY_WINDOW_S, REASON_ABOVE_CAP, REASON_ZERO, THRESHOLD_COUNT,
    threshold,
};
use crate::config::raw;
use crate::error::ErrorKind;

/// The reason `value` is refused for under `cap`, or `None` when it is kept
/// (or refused as anything but a threshold, which every case here rules out).
fn refused(value: u32, cap: u32) -> Option<&'static str> {
    threshold(THRESHOLD_COUNT, Some(value), cap)
        .err()
        .and_then(|failure| match failure.kind() {
            ErrorKind::InvalidThreshold { reason, .. } => Some(*reason),
            _ => None,
        })
}

#[test]
fn a_threshold_at_its_cap_is_kept() {
    assert_eq!(refused(MAX_ANOMALY_REPEATS, MAX_ANOMALY_REPEATS), None);
    assert_eq!(refused(MAX_ANOMALY_WINDOW_S, MAX_ANOMALY_WINDOW_S), None);
}

#[test]
fn a_threshold_one_past_its_cap_is_refused_as_above_it() {
    assert_eq!(
        refused(MAX_ANOMALY_REPEATS + 1, MAX_ANOMALY_REPEATS),
        Some(REASON_ABOVE_CAP)
    );
}

#[test]
fn a_zero_threshold_is_refused_as_zero_not_as_above_the_cap() {
    assert_eq!(refused(0, MAX_ANOMALY_REPEATS), Some(REASON_ZERO));
}

#[test]
fn a_kept_threshold_keeps_its_value() {
    let kept = threshold(THRESHOLD_COUNT, Some(3), MAX_ANOMALY_REPEATS)
        .map(core::num::NonZeroU32::get)
        .ok();

    assert_eq!(kept, Some(3));
}

#[test]
fn a_rule_missing_its_pattern_is_refused_for_that_before_its_zero_threshold() {
    let authored: raw::AnomalyRule =
        serde_json::from_str(r#"{"threshold_count": 0, "threshold_window_s": 60}"#)
            .expect("the rule parses as a shape");

    let failure =
        super::AnomalyRule::try_from(authored).expect_err("a rule with no pattern is refused");

    assert!(
        matches!(failure.kind(), ErrorKind::MissingRequiredField { field } if *field == "pattern"),
        "{failure:?}"
    );
}
