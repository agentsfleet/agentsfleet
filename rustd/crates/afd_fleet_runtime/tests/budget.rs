//! Decimal-string ceilings are exact, bounded and reject legacy/ambiguous input.

use afd_fleet_runtime::FleetConfig;
use serde_json::{Value, json};

fn document(budget: &Value) -> String {
    json!({"name":"probe", "x-agentsfleet": {
        "triggers":[], "tools":[], "budget":budget
    }})
    .to_string()
}

#[test]
fn test_nanos_accepts_positive_daily_and_optional_monthly_boundaries() {
    // pin test: literal is the contract — the documented daily range is 1..1000000000000.
    for daily in ["1", "999999999999", "1000000000000"] {
        let parsed = FleetConfig::authored(&document(&json!({"daily_nanos":daily})));
        assert!(parsed.is_ok(), "positive ceiling {daily}: {parsed:?}");
    }
    for monthly in ["1", "10000000000000"] {
        let parsed = FleetConfig::authored(&document(&json!({
            "daily_nanos":"1", "monthly_nanos":monthly
        })));
        assert!(parsed.is_ok(), "monthly ceiling {monthly}: {parsed:?}");
    }
}

#[test]
fn test_nanos_rejects_zero_signs_fractions_exponents_and_overflow() {
    for daily in [
        "0",
        "-1",
        "+1",
        "1.0",
        "1e9",
        "",
        " 1",
        "1 ",
        "١",
        "1000000000001",
        "9223372036854775808",
    ] {
        let parsed = FleetConfig::authored(&document(&json!({"daily_nanos":daily})));
        assert!(parsed.is_err(), "invalid ceiling {daily:?} was accepted");
    }
    for monthly in ["0", "-1", "+1", "1.5", "1e9", "10000000000001"] {
        assert!(
            FleetConfig::authored(&document(&json!({
                "daily_nanos":"1", "monthly_nanos":monthly
            })))
            .is_err(),
            "invalid monthly ceiling {monthly:?}"
        );
    }
}

#[test]
fn test_nanos_rejects_numbers_null_missing_daily_and_legacy_keys() {
    for budget in [
        json!({"daily_nanos":1}),
        json!({"daily_nanos":null}),
        json!({"monthly_nanos":"1"}),
        json!({"daily_dollars":1}),
        json!({"daily_nanos":"1","daily_dollars":1}),
        json!({"daily_nanos":"1","monthly_dollars":1}),
        json!({"daily_nanos":"1","monthly_nanos":2}),
        json!({"daily_nanos":"1","monthly_nanos":null}),
    ] {
        assert!(
            FleetConfig::authored(&document(&budget)).is_err(),
            "invalid budget {budget} was accepted"
        );
    }
}
