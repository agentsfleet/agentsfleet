//! Policy shapes as the published document and the wire carry them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::policy::{HttpMethod, HttpPathMatch, HttpRequestRule};

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

/// A blob write rule carrying `permitted_fields` as given.
fn blob_rule(permitted_fields: Option<Vec<Cow<'static, str>>>) -> HttpRequestRule<'static> {
    HttpRequestRule {
        method: HttpMethod::Post,
        path: Cow::Borrowed("/repos/acme/widgets/git/blobs"),
        path_match: HttpPathMatch::Exact,
        json_fields: Vec::new(),
        permitted_fields,
    }
}

/// An open rule goes on the wire without `permitted_fields`, as the field's
/// description says, and a closed one carries its list, empty included. A
/// `null` from a writer that sends one still reads as open.
#[test]
fn test_an_open_rule_omits_permitted_fields_and_a_closed_one_carries_it() {
    let encoded =
        |rule: HttpRequestRule<'static>| serde_json::to_value(rule).expect("a rule encodes");

    let open = encoded(blob_rule(None));
    assert!(open.get("permitted_fields").is_none(), "{open}");
    let shut = encoded(blob_rule(Some(Vec::new())));
    assert_eq!(shut["permitted_fields"], serde_json::json!([]));
    let listed = encoded(blob_rule(Some(vec![Cow::Borrowed("content")])));
    assert_eq!(listed["permitted_fields"], serde_json::json!(["content"]));

    let nulled: HttpRequestRule<'_> = serde_json::from_str(
        r#"{"method":"post","path":"/x","path_match":"exact","json_fields":[],"permitted_fields":null}"#,
    )
    .expect("a null list decodes");
    assert_eq!(nulled.permitted_fields, None);
}
