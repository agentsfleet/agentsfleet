#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use regex::Regex;
use serde::Serialize;

use super::PATTERN;

/// A body carrying one signed amount, as the billing snapshot does.
#[derive(Debug, Serialize)]
struct Balance {
    #[serde(with = "crate::nanos::signed")]
    balance: i64,
}

/// The published pattern, compiled.
fn pattern() -> Regex {
    Regex::new(PATTERN).expect("the published pattern compiles")
}

/// The decimal string `balance` is written as.
fn written(balance: i64) -> String {
    let json = serde_json::to_value(Balance { balance }).expect("serializes");
    json.get("balance")
        .and_then(serde_json::Value::as_str)
        .expect("the amount is a string")
        .to_owned()
}

#[test]
fn test_a_debt_travels_as_a_negative_decimal_string() {
    for (balance, text) in [
        (-50, "-50"),
        (-1, "-1"),
        (0, "0"),
        (25_000, "25000"),
        (i64::MIN, "-9223372036854775808"),
        (i64::MAX, "9223372036854775807"),
    ] {
        assert_eq!(written(balance), text, "{balance}");
        assert!(
            pattern().is_match(text),
            "{text} matches the published pattern"
        );
    }
}

#[test]
fn test_the_pattern_refuses_negative_zero_and_every_other_non_canonical_spelling() {
    for text in [
        "-0", "-00", "-01", "00", "01", "+1", "", "-", " -1", "-1 ", "1.0", "--1",
    ] {
        assert!(!pattern().is_match(text), "{text:?} must not match");
    }
}

#[test]
fn test_the_signed_pattern_admits_every_unsigned_spelling() {
    let unsigned = Regex::new(crate::nanos::PATTERN).expect("the unsigned pattern compiles");
    for text in ["0", "7", "3000000", "9223372036854775807"] {
        assert!(unsigned.is_match(text), "{text}");
        assert!(pattern().is_match(text), "{text} stays valid once signed");
    }
}

#[cfg(feature = "openapi")]
#[test]
fn test_the_published_shape_is_a_signed_patterned_string() {
    use utoipa::{PartialSchema as _, ToSchema as _};
    let schema = serde_json::to_value(super::SignedDecimal::schema()).expect("serializes");
    assert_eq!(schema.get("type"), Some(&serde_json::json!("string")));
    assert_eq!(schema.get("pattern"), Some(&serde_json::json!(PATTERN)));
    assert_eq!(super::SignedDecimal::name(), "SignedNanos");
}
