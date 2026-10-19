#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use serde::{Deserialize, Serialize};

use super::{NOT_CANONICAL, TOO_LARGE, parse};

/// A body carrying one required and one optional amount.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Amounts {
    #[serde(with = "crate::nanos")]
    total: i64,
    #[serde(with = "crate::nanos::option", default)]
    rate: Option<i64>,
}

#[test]
fn test_an_amount_travels_as_a_decimal_string_and_back() {
    for (amounts, json) in [
        (
            Amounts {
                total: 0,
                rate: None,
            },
            r#"{"total":"0","rate":null}"#,
        ),
        (
            Amounts {
                total: 9_007_199_254_740_993,
                rate: Some(3_000_000),
            },
            r#"{"total":"9007199254740993","rate":"3000000"}"#,
        ),
        (
            Amounts {
                total: i64::MAX,
                rate: Some(0),
            },
            r#"{"total":"9223372036854775807","rate":"0"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&amounts).expect("serializes"), json);
        let back: Amounts = serde_json::from_str(json).expect("parses");
        assert_eq!(back, amounts, "{json} round-trips exactly");
    }
}

#[test]
fn test_a_non_canonical_spelling_is_refused() {
    for (text, refusal) in [
        ("", NOT_CANONICAL),
        ("-1", NOT_CANONICAL),
        ("+1", NOT_CANONICAL),
        ("01", NOT_CANONICAL),
        ("00", NOT_CANONICAL),
        (" 1", NOT_CANONICAL),
        ("1 ", NOT_CANONICAL),
        ("1.0", NOT_CANONICAL),
        ("1e3", NOT_CANONICAL),
        ("١", NOT_CANONICAL),
        ("9223372036854775808", TOO_LARGE),
        ("99999999999999999999", TOO_LARGE),
    ] {
        assert_eq!(parse(text), Err(refusal), "{text:?}");
    }
}

#[test]
fn test_a_json_number_is_refused_where_a_string_belongs() {
    for json in [r#"{"total":5}"#, r#"{"total":"5","rate":7}"#] {
        let refused = serde_json::from_str::<Amounts>(json)
            .map(|_| ())
            .map_err(|_refused| ());
        assert_eq!(refused, Err(()), "{json} must not decode");
    }
}

#[test]
fn test_a_refusal_names_the_rule() {
    let error = serde_json::from_str::<Amounts>(r#"{"total":"007"}"#).expect_err("refused");
    assert!(error.to_string().contains(NOT_CANONICAL), "{error}");
}

#[cfg(feature = "openapi")]
#[test]
fn test_the_published_shape_is_a_patterned_string() {
    use utoipa::{PartialSchema as _, ToSchema as _};
    let schema = serde_json::to_value(super::Decimal::schema()).expect("serializes");
    assert_eq!(schema.get("type"), Some(&serde_json::json!("string")));
    assert_eq!(
        schema.get("pattern"),
        Some(&serde_json::json!(super::PATTERN))
    );
    assert_eq!(super::Decimal::name(), "Nanos");
}
