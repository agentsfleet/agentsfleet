//! Decimal-string ceilings are exact, positive and reject legacy/ambiguous input.

use af_core::money::NANOS_PER_USD;
use af_fleet_runtime::FleetConfig;
use serde_json::{Value, json};

fn document(budget: &Value) -> String {
    json!({"name":"probe", "x-agentsfleet": {
        "triggers":[], "tools":[], "budget":budget
    }})
    .to_string()
}

/// A fleet's own budget is its only ceiling: $50,000 a day is accepted when
/// authored, stored as written, and read back.
#[test]
fn a_fleet_budget_has_no_ceiling_but_its_own() -> Result<(), Box<dyn std::error::Error>> {
    let daily = 50_000 * NANOS_PER_USD;
    let authored = FleetConfig::authored(&document(&json!({"daily_nanos":daily.to_string()})))?;
    let stored = serde_json::to_value(authored.budget())?;
    assert_eq!(stored, json!({"daily_nanos":daily.to_string()}));
    let read_back = FleetConfig::stored(&document(&stored))?;
    assert_eq!(read_back.budget().daily().nanos(), daily);
    Ok(())
}

#[test]
fn test_nanos_accepts_positive_daily_and_optional_monthly_boundaries() {
    // pin test: literal is the contract — a ceiling is any positive i64 of nanos.
    for daily in ["1", "1000000000001", "9223372036854775807"] {
        let parsed = FleetConfig::authored(&document(&json!({"daily_nanos":daily})));
        assert!(parsed.is_ok(), "positive ceiling {daily}: {parsed:?}");
    }
    for monthly in ["1", "10000000000001", "9223372036854775807"] {
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
        "9223372036854775808",
    ] {
        let parsed = FleetConfig::authored(&document(&json!({"daily_nanos":daily})));
        assert!(parsed.is_err(), "invalid ceiling {daily:?} was accepted");
    }
    for monthly in ["0", "-1", "+1", "1.5", "1e9", "9223372036854775808"] {
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
