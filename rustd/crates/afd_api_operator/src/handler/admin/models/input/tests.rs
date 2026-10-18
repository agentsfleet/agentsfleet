#![expect(
    clippy::expect_used,
    reason = "test fixtures must fail loudly when malformed"
)]

use super::*;

/// The instant every fixture's evidence was verified at.
const VERIFIED: &str = "2026-10-18T00:00:00Z";

/// A complete create body: evidence and one open-ended base tier.
fn valid() -> serde_json::Value {
    serde_json::json!({
        (FIELD_PROVIDER): "anthropic",
        (FIELD_MODEL_ID): "claude-opus-5",
        "metadata": {
            "display_name": "Opus 5",
            "context_cap_tokens": 200_000,
            "context_source_url": "https://example.test/models",
            "context_verified_at": VERIFIED,
            "advanced": false
        },
        "prices": [price(0, "2026-10-01T00:00:00Z", None)]
    })
}

fn price(tier: i64, from: &str, until: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "effective_from": from,
        "effective_until": until,
        "prompt_tokens_above": tier,
        "input_nanos_per_mtok": "5",
        "cached_input_nanos_per_mtok": "1",
        "cache_write_nanos_per_mtok": null,
        "cache_write_5m_nanos_per_mtok": "6",
        "cache_write_1h_nanos_per_mtok": null,
        "output_nanos_per_mtok": "25",
        "source_url": "https://example.test/pricing",
        "verified_at": VERIFIED,
        "conditions": ""
    })
}

fn create(body: &serde_json::Value) -> Result<(), &'static str> {
    create_request(&serde_json::to_vec(body).expect("body JSON")).map(|_valid| ())
}

fn patch(body: &serde_json::Value) -> Result<(), &'static str> {
    let mut patch = body.clone();
    let object = patch.as_object_mut().expect("object");
    object.remove(FIELD_PROVIDER);
    object.remove(FIELD_MODEL_ID);
    patch_request(&serde_json::to_vec(&patch).expect("patch JSON")).map(|_valid| ())
}

fn with(mut body: serde_json::Value, pointer: &str, value: serde_json::Value) -> serde_json::Value {
    *body.pointer_mut(pointer).expect("fixture field") = value;
    body
}

#[test]
fn create_validation_pins_every_bound() {
    assert_eq!(create(&valid()), Ok(()));
    assert_eq!(
        create_request(b"").map(|_valid| ()),
        Err(DETAIL_BODY_REQUIRED)
    );
    assert_eq!(
        create_request(b"[]").map(|_valid| ()),
        Err(DETAIL_MALFORMED_JSON)
    );
    for (pointer, value, expected) in [
        ("/provider", serde_json::json!(""), DETAIL_PROVIDER_LEN),
        (
            "/metadata/context_cap_tokens",
            serde_json::json!(0),
            DETAIL_CAP_POSITIVE,
        ),
        (
            "/metadata/display_name",
            serde_json::json!(""),
            DETAIL_DISPLAY_NAME_LEN,
        ),
        (
            "/metadata/context_source_url",
            serde_json::json!(""),
            DETAIL_CONTEXT_SOURCE_LEN,
        ),
        (
            "/prices/0/input_nanos_per_mtok",
            serde_json::json!(-1),
            DETAIL_RATES_NONNEGATIVE,
        ),
        (
            "/prices/0/cache_write_1h_nanos_per_mtok",
            serde_json::json!(-1),
            DETAIL_RATES_NONNEGATIVE,
        ),
        (
            "/prices/0/prompt_tokens_above",
            serde_json::json!(-1),
            DETAIL_PRICE_TIER,
        ),
        (
            "/prices/0/source_url",
            serde_json::json!(""),
            DETAIL_PRICE_SOURCE_LEN,
        ),
        (
            "/prices/0/conditions",
            serde_json::json!("x".repeat(1025)),
            DETAIL_PRICE_CONDITIONS_LEN,
        ),
        ("/prices", serde_json::json!([]), DETAIL_PRICES),
    ] {
        let body = with(valid(), pointer, value);
        assert_eq!(create(&body), Err(expected), "{pointer}");
        if !pointer.starts_with("/provider") {
            assert_eq!(patch(&body), Err(expected), "{pointer} on a patch");
        }
    }
}

// Value: protects=two quotes for one tier can never both apply at one instant;
// fails_when=an overlapping or reversed range, or a schedule with no base tier,
// reaches the store; why_new=§8 dated prices; seam=none; no_claim=storage.
#[test]
fn a_schedule_with_overlapping_reversed_or_no_base_ranges_is_refused() {
    for prices in [
        vec![
            price(0, "2026-10-01T00:00:00Z", None),
            price(0, "2026-11-01T00:00:00Z", None),
        ],
        vec![
            price(0, "2026-10-01T00:00:00Z", Some("2026-12-01T00:00:00Z")),
            price(0, "2026-11-01T00:00:00Z", None),
        ],
        vec![price(
            0,
            "2026-10-01T00:00:00Z",
            Some("2026-09-01T00:00:00Z"),
        )],
        vec![price(200_000, "2026-10-01T00:00:00Z", None)],
    ] {
        let body = with(valid(), "/prices", serde_json::Value::Array(prices));
        assert_eq!(create(&body), Err(DETAIL_PRICES));
        assert_eq!(patch(&body), Err(DETAIL_PRICES));
    }
    let successive = with(
        valid(),
        "/prices",
        serde_json::json!([
            price(0, "2026-10-01T00:00:00Z", Some("2026-11-01T00:00:00Z")),
            price(0, "2026-11-01T00:00:00Z", None),
            price(200_000, "2026-10-01T00:00:00Z", None),
        ]),
    );
    assert_eq!(create(&successive), Ok(()));
}

#[test]
fn inference_validation_names_the_field_in_create_and_patch() {
    for (field, invalid, expected) in [
        ("base_url", serde_json::json!(""), DETAIL_INFERENCE_URL),
        ("dialect", serde_json::json!(""), DETAIL_INFERENCE_DIALECT),
        (
            "max_output_tokens",
            serde_json::json!(0),
            DETAIL_INFERENCE_OUTPUT,
        ),
    ] {
        let mut inference = serde_json::json!({"api":"chat","base_url":"https://models.example/v1","max_output_tokens":4096});
        inference
            .as_object_mut()
            .expect("inference object")
            .insert(field.into(), invalid);
        let mut body = valid();
        body.as_object_mut()
            .expect("body object")
            .insert("inference".into(), inference);
        assert_eq!(create(&body), Err(expected));
        assert_eq!(patch(&body), Err(expected));
    }
}

#[test]
fn patch_requires_a_bounded_body() {
    assert_eq!(
        patch_request(b"").map(|_valid| ()),
        Err(DETAIL_BODY_REQUIRED)
    );
    assert_eq!(
        patch_request(b"[]").map(|_valid| ()),
        Err(DETAIL_MALFORMED_JSON)
    );
    assert_eq!(patch(&valid()), Ok(()));
}
