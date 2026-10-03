//! What a platform-key body and path refuse, and the sentence each break earns.

use super::*;

const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02";

#[test]
fn request_validation_pins_pairing_and_bounds() {
    let named = format!(
        r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE}","model":"claude-opus-5","base_url":null}}"#
    );
    assert_eq!(request(named.as_bytes()).map(|_request| ()), Ok(()));

    let compatible = format!(
        r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE}","model":"custom","base_url":"https://models.example/v1"}}"#
    );
    assert_eq!(request(compatible.as_bytes()).map(|_request| ()), Ok(()));
    assert_eq!(
        request(b""),
        Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED))
    );
    assert_eq!(
        request(b"[]"),
        Err((error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))
    );

    let unsafe_url = format!(
        r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE}","model":"custom","base_url":"https://127.0.0.1/v1"}}"#
    );
    assert_eq!(
        request(unsafe_url.as_bytes()),
        Err((error_code::PROVIDER_BASE_URL_INVALID, DETAIL_BASE_URL))
    );

    let credential_url = format!(
        r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE}","model":"custom","base_url":"https://user:password@models.example/v1"}}"#
    );
    assert_eq!(
        request(credential_url.as_bytes()),
        Err((error_code::PROVIDER_BASE_URL_INVALID, DETAIL_BASE_URL))
    );
}

/// The sentence names the field whose bound broke, keyed by the path
/// `garde` reports — the wording is a public contract the dashboard
/// renders, so each of the two bounds must earn its own sentence.
#[test]
fn a_broken_bound_is_told_as_the_field_that_broke_it() {
    // The caps are read from the wire type that DECLARES them, never
    // copied: a local number would let the bound move while these cases
    // asserted the old one and still passed.
    let provider_past_cap = format!(
        r#"{{"provider":"{}","source_workspace_id":"{WORKSPACE}","model":"claude-opus-5","base_url":null}}"#,
        "p".repeat(KEY_PROVIDER_MAX_BYTES + 1)
    );
    assert_eq!(
        request(provider_past_cap.as_bytes()),
        Err((error_code::INVALID_REQUEST, DETAIL_PROVIDER_LEN))
    );

    let provider_empty = format!(
        r#"{{"provider":"","source_workspace_id":"{WORKSPACE}","model":"claude-opus-5","base_url":null}}"#
    );
    assert_eq!(
        request(provider_empty.as_bytes()),
        Err((error_code::INVALID_REQUEST, DETAIL_PROVIDER_LEN))
    );

    let model_past_cap = format!(
        r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE}","model":"{}","base_url":null}}"#,
        "m".repeat(afd_wire::admin::MODEL_ID_MAX_BYTES + 1)
    );
    assert_eq!(
        request(model_past_cap.as_bytes()),
        Err((error_code::INVALID_REQUEST, DETAIL_MODEL_LEN))
    );

    // A report with nothing in it cannot name a field. `validate` never
    // produces one, so the default is driven directly: the fallback must
    // stay a refusal rather than a sentence blaming a field that passed.
    assert_eq!(BOUNDS.pick(&garde::Report::new()), DETAIL_MALFORMED_JSON);
}

/// The `DELETE` path's provider is held to the body's bound, and a break
/// earns the body's sentence.
#[test]
fn a_provider_path_segment_is_bounded_like_the_body() {
    let picked = |length: usize| {
        ProviderSegment {
            provider: "p".repeat(length),
        }
        .validate()
        .err()
        .map(|report| BOUNDS.pick(&report))
    };
    assert_eq!(picked(KEY_PROVIDER_MAX_BYTES), None);
    for refused in [0, KEY_PROVIDER_MAX_BYTES + 1] {
        assert_eq!(
            picked(refused),
            Some(DETAIL_PROVIDER_LEN),
            "{refused} bytes"
        );
    }
}
