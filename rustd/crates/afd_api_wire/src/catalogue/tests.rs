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
fn a_negative_rate_or_tier_breaks_its_own_bound() {
    let mut negative = price(BASE_TIER, 0, None);
    negative.cache_write_1h_nanos_per_mtok = Some(-1);
    assert!(negative.validate().is_err());
    assert!(price(-1, 0, None).validate().is_err());
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
