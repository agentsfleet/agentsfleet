//! The context budget as the published document describes it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

/// The stage fraction is published as what the parser enforces: a fraction of
/// the context window, 0 to 1, so a reader never writes a percentage.
#[test]
fn test_the_published_stage_chunk_threshold_is_a_unit_fraction() {
    let openapi = include_str!("../../../../public/openapi.json");
    let document: serde_json::Value =
        serde_json::from_str(openapi).expect("the published document is JSON");
    let bound = |name: &str| {
        document
            .pointer(&format!(
                "/components/schemas/ContextBudget/properties/stage_chunk_threshold/{name}"
            ))
            .and_then(serde_json::Value::as_f64)
    };

    assert_eq!((bound("minimum"), bound("maximum")), (Some(0.0), Some(1.0)));
}
