//! Policy shapes as the derived schema and the wire carry them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::policy::{HttpEgress, HttpMethod, HttpPathMatch, HttpRequestRule, NetworkPolicy};

/// The stage fraction is published as what the parser enforces: a fraction of
/// the context window, 0 to 1, so a reader never writes a percentage.
#[cfg(feature = "openapi")]
#[test]
fn test_the_published_stage_chunk_threshold_is_a_unit_fraction() {
    let schema =
        serde_json::to_value(<afd_wire::policy::ContextBudget as utoipa::PartialSchema>::schema())
            .expect("the derived schema serializes");
    let bound = |name: &str| {
        schema
            .pointer(&format!("/properties/stage_chunk_threshold/{name}"))
            .and_then(serde_json::Value::as_f64)
    };

    assert_eq!(
        (bound("minimum"), bound("maximum")),
        (Some(0.0), Some(1.0)),
        "{schema}"
    );
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
    assert_eq!(shut.get("permitted_fields"), Some(&serde_json::json!([])));
    let listed = encoded(blob_rule(Some(vec![Cow::Borrowed("content")])));
    assert_eq!(
        listed.get("permitted_fields"),
        Some(&serde_json::json!(["content"]))
    );

    let nulled: HttpRequestRule<'_> = serde_json::from_str(
        r#"{"method":"post","path":"/x","path_match":"exact","json_fields":[],"permitted_fields":null}"#,
    )
    .expect("a null list decodes");
    assert_eq!(nulled.permitted_fields, None);
}

// Value: protects=older policy payloads retain restricted HTTP destinations;
// fails_when=an omitted setting widens access or a misspelled mode is accepted.
#[test]
fn http_destination_scope_defaults_and_round_trips() {
    let payload = serde_json::json!({"allow":[],"read_only":true,"read_post_paths":[]});
    let encoded = payload.to_string();
    let restricted: NetworkPolicy<'_> = serde_json::from_str(&encoded).expect("old payload");
    assert_eq!(restricted.http_egress, HttpEgress::Restricted);
    assert!(!restricted.package_manager_domains);
    for mode in [HttpEgress::Restricted, HttpEgress::Public] {
        let payload = serde_json::json!({"allow":[],"read_only":true,"read_post_paths":[],
            "http_egress":mode,"package_manager_domains":true});
        let encoded = payload.to_string();
        let policy: NetworkPolicy<'_> = serde_json::from_str(&encoded).expect("HTTP policy");
        assert_eq!(policy.http_egress, mode);
        assert!(policy.package_manager_domains);
        assert_eq!(serde_json::to_value(policy).expect("round trip"), payload);
    }
    let invalid = serde_json::json!({"allow":[],"read_only":true,"read_post_paths":[],"http_egress":"unknown"});
    serde_json::from_str::<NetworkPolicy<'_>>(&invalid.to_string()).expect_err("unknown mode");
}
