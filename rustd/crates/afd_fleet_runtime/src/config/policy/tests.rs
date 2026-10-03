//! A spend ceiling at each rule garde proves it against, in rule order.

use afd_core::error_code;

use super::{
    DAILY_DOLLARS, Dollars, MAX_DAILY_DOLLARS, REASON_ABOVE_CAP, REASON_NOT_FINITE,
    REASON_NOT_POSITIVE,
};

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
