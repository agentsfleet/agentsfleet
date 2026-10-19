//! The price schedule's bounds and the wire spelling of both shapes.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::borrow::Cow;

use garde::Validate as _;

use super::{BASE_TIER, CatalogueMetadata, CataloguePrice, NO_BASE_TIER, price_schedule};

const DAY: i64 = 86_400;
const START: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);
const TIER: i32 = 200_000;

fn price(tier: i32, from: i64, until: Option<i64>) -> CataloguePrice<'static> {
    CataloguePrice {
        effective_from: jiff::Timestamp::constant(START.as_second() + from, 0),
        effective_until: until.map(|until| jiff::Timestamp::constant(START.as_second() + until, 0)),
        prompt_tokens_above: tier,
        input_nanos_per_mtok: 3_000_000_000,
        cached_input_nanos_per_mtok: 300_000_000,
        cache_write_nanos_per_mtok: None,
        cache_write_5m_nanos_per_mtok: Some(3_750_000_000),
        cache_write_1h_nanos_per_mtok: None,
        output_nanos_per_mtok: 15_000_000_000,
        source_url: Cow::Borrowed("https://example.test/pricing"),
        verified_at: START,
        conditions: Cow::Borrowed(""),
    }
}

fn refusal(prices: &[CataloguePrice<'_>]) -> Option<String> {
    price_schedule(prices, &())
        .err()
        .map(|error| error.message().to_owned())
}

#[test]
fn a_base_tier_with_dated_successors_and_a_higher_tier_is_a_schedule() {
    let prices = [
        price(BASE_TIER, 0, Some(DAY)),
        price(BASE_TIER, DAY, None),
        price(TIER, 0, None),
    ];
    assert_eq!(refusal(&prices), None);
    assert!(prices.iter().all(|price| price.validate().is_ok()));
}

#[test]
fn a_schedule_needs_a_base_tier_and_disjoint_ranges() {
    assert_eq!(
        refusal(&[price(TIER, 0, None)]).as_deref(),
        Some(NO_BASE_TIER)
    );
    assert_eq!(refusal(&[]).as_deref(), Some(NO_BASE_TIER));
    assert!(refusal(&[price(BASE_TIER, 0, None), price(BASE_TIER, DAY, None)]).is_some());
    assert!(refusal(&[price(BASE_TIER, DAY, Some(0))]).is_some());
}

#[test]
fn a_negative_tier_breaks_its_bound_and_a_negative_rate_never_decodes() {
    assert!(price(-1, 0, None).validate().is_err());
    let mut wire = serde_json::to_value(price(BASE_TIER, 0, None)).expect("a price serializes");
    wire["cache_write_1h_nanos_per_mtok"] = serde_json::json!("-1");
    let text = wire.to_string();
    let refusal = serde_json::from_str::<CataloguePrice<'_>>(&text)
        .map(|_| ())
        .expect_err("a negative rate never decodes");
    assert!(refusal.to_string().contains(afd_wire::nanos::NOT_CANONICAL));
}

#[test]
fn an_unknown_cache_write_is_null_and_instants_are_rfc_3339() {
    let wire = serde_json::to_value(price(BASE_TIER, 0, None)).expect("a price serializes");
    assert_eq!(wire["cache_write_nanos_per_mtok"], serde_json::Value::Null);
    assert_eq!(wire["effective_until"], serde_json::Value::Null);
    assert_eq!(wire["effective_from"], "2025-10-09T08:53:20Z");
    let metadata = CatalogueMetadata {
        display_name: Cow::Borrowed(""),
        context_cap_tokens: 0,
        context_source_url: Cow::Borrowed("https://example.test/models"),
        context_verified_at: START,
        advanced: true,
    };
    assert!(
        metadata.validate().is_err(),
        "an empty label and a zero cap are refused"
    );
}

/// Postgres refuses NUL inside `text`, so every catalogue string an operator
/// types must be the caller's 400 rather than the store's 500.
#[test]
fn a_nul_in_any_catalogue_text_is_refused_at_the_boundary() {
    let refused = |report: garde::Report| report.to_string().contains(afd_validate::rules::HAS_NUL);
    let metadata = |display_name: &'static str, url: &'static str| CatalogueMetadata {
        display_name: Cow::Borrowed(display_name),
        context_cap_tokens: 200_000,
        context_source_url: Cow::Borrowed(url),
        context_verified_at: START,
        advanced: false,
    };
    metadata("Opus", "https://example.test/models")
        .validate()
        .expect("clean metadata passes");
    for broken in [
        metadata("Op\0us", "https://example.test/models"),
        metadata("Opus", "https://example.test/\0models"),
    ] {
        assert!(refused(broken.validate().expect_err("a NUL is refused")));
    }

    let mut price = price(BASE_TIER, 0, None);
    price.conditions = Cow::Borrowed("batch\0only");
    assert!(refused(
        price.validate().expect_err("a NUL condition is refused")
    ));
    price.conditions = Cow::Borrowed("");
    price.source_url = Cow::Borrowed("https://example.test/\0pricing");
    assert!(refused(
        price.validate().expect_err("a NUL source is refused")
    ));
}
