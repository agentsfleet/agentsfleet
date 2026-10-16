#![expect(
    clippy::expect_used,
    reason = "each input is built to be refused; an acceptance is the failure"
)]

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::Error as _;
use serde_json::error::Category;

use super::Unreadable;

/// A value that must never appear in a rendering, whatever serde did with it.
const SECRET: &str = "sk-live-private-instructions, expected nothing";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "kind", default)]
    _kind: Option<Kind>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Webhook,
}

/// A type whose own `Deserialize` quotes what it refused.
#[derive(Debug)]
struct Quoting;

impl<'de> Deserialize<'de> for Quoting {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Err(D::Error::custom(format!("refused {value}")))
    }
}

fn unreadable<T: for<'de> Deserialize<'de> + std::fmt::Debug>(input: &str) -> Unreadable {
    serde_json::from_str::<T>(input)
        .expect_err("the input is built to be refused")
        .into()
}

fn assert_private(refusal: &Unreadable) {
    for rendered in [refusal.to_string(), format!("{refusal:?}")] {
        assert!(!rendered.contains("sk-live"), "leaked: {rendered}");
    }
    assert!(std::error::Error::source(refusal).is_none());
}

#[test]
fn a_wrong_typed_string_keeps_its_kind_position_and_expected_type_but_not_its_text() {
    let refusal = unreadable::<Setup>(&serde_json::Value::from(SECRET).to_string());
    assert_private(&refusal);
    assert_eq!(refusal.category(), Category::Data);
    assert_eq!(
        refusal.to_string(),
        format!(
            "invalid type: string, expected struct Setup at line 1 column {}",
            refusal.column()
        )
    );
}

#[test]
fn a_refused_value_and_an_unknown_variant_drop_what_the_document_supplied() {
    let value = unreadable::<char>(&serde_json::Value::from(SECRET).to_string());
    assert_private(&value);
    assert!(
        value
            .to_string()
            .starts_with("invalid value: string, expected a character")
    );

    let variant = unreadable::<Setup>(&format!(r#"{{"name":"a","kind":"{SECRET}"}}"#));
    assert_private(&variant);
    assert!(
        variant
            .to_string()
            .starts_with("unknown variant, expected variant identifier")
            || variant
                .to_string()
                .starts_with("unknown variant, expected `webhook`"),
        "{variant}"
    );
}

#[test]
fn a_refusal_a_type_worded_itself_is_dropped_whole() {
    let refusal = unreadable::<Quoting>(&serde_json::Value::from(SECRET).to_string());
    assert_private(&refusal);
    assert!(refusal.to_string().starts_with(super::UNRECOGNISED));
}

#[test]
fn schema_names_stay_and_an_unknown_key_stays_only_inside_its_bound() {
    let missing = unreadable::<Setup>("{}");
    assert!(
        missing.to_string().starts_with("missing field `name`"),
        "{missing}"
    );

    let short = unreadable::<Setup>(r#"{"name":"a","gatess":1}"#);
    assert!(
        short
            .to_string()
            .starts_with("unknown field `gatess`, expected `name` or `kind`")
    );

    let long_key = "k".repeat(65);
    let long = unreadable::<Setup>(&format!(r#"{{"name":"a","{long_key}":1}}"#));
    assert!(!long.to_string().contains(&long_key), "{long}");
    assert!(
        long.to_string()
            .starts_with("unknown field, expected `name` or `kind`")
    );
}

#[test]
fn a_wrong_length_keeps_its_count() {
    let refusal = unreadable::<[u8; 2]>("[1]");
    assert!(
        refusal
            .to_string()
            .starts_with("invalid length 1, expected"),
        "{refusal}"
    );
}

#[test]
fn syntax_and_early_end_keep_serdes_fixed_wording_and_position() {
    let syntax = unreadable::<Setup>("# Instructions\nname: x\n");
    assert_eq!(syntax.category(), Category::Syntax);
    assert_eq!(syntax.to_string(), "expected value at line 1 column 1");

    let early = unreadable::<BTreeMap<String, String>>(r#"{"name":"#);
    assert_eq!(early.category(), Category::Eof);
    assert_eq!((early.line(), early.column()), (1, 8));
    assert!(early.to_string().starts_with("EOF while parsing a value"));
}

#[test]
fn an_unpositioned_refusal_renders_without_a_position() {
    let refusal: Unreadable = serde_json::from_value::<Setup>(serde_json::Value::from(SECRET))
        .expect_err("a string is not a setup")
        .into();
    assert_private(&refusal);
    assert_eq!(refusal.line(), 0);
    assert_eq!(
        refusal.to_string(),
        "invalid type: string, expected struct Setup"
    );
}
