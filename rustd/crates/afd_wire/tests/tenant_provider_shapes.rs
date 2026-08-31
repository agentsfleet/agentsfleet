//! The tenant provider surface's frozen shape, in both directions.
//!
//! The read is what a dashboard renders and the write is what it sends, so
//! these pin the bytes rather than the fields: a rename, a null where an
//! omission belongs, or an accepted stray field are all silent breaks that
//! field-level equality would miss.
#![expect(clippy::expect_used, reason = "tests inspect JSON documents")]

use afd_wire::tenant_provider::{ProviderMode, TenantProviderRequest, TenantProviderResponse};

/// The credential name used wherever a well-formed one is needed.
const A_KEY: &str = "openai-primary";

#[test]
fn platform_mode_omits_the_credential_rather_than_nulling_it() {
    let rendered = serde_json::to_string(&TenantProviderResponse {
        mode: ProviderMode::Platform,
        provider: "anthropic",
        model: "claude-opus-5",
        context_cap_tokens: 200_000,
        secret_ref: None,
        platform_default_available: true,
    })
    .expect("the response serializes");

    assert!(
        !rendered.contains("secret_ref"),
        "an absent credential is an absent FIELD, not a null one: {rendered}"
    );
    assert!(rendered.contains(r#""mode":"platform""#), "{rendered}");
}

#[test]
fn self_managed_mode_names_the_credential_it_dials() {
    let rendered = serde_json::to_string(&TenantProviderResponse {
        mode: ProviderMode::SelfManaged,
        provider: "openai",
        model: "gpt-5",
        context_cap_tokens: 128_000,
        secret_ref: Some(A_KEY),
        platform_default_available: false,
    })
    .expect("the response serializes");

    assert!(rendered.contains(r#""mode":"self_managed""#), "{rendered}");
    assert!(
        rendered.contains(r#""secret_ref":"openai-primary""#),
        "{rendered}"
    );
}

#[test]
fn the_response_never_carries_a_field_shaped_like_a_key() {
    // The whole surface's guarantee in one assertion. `secret_ref` is a LABEL;
    // nothing here may carry what the credential contains.
    let rendered = serde_json::to_string(&TenantProviderResponse {
        mode: ProviderMode::SelfManaged,
        provider: "openai",
        model: "gpt-5",
        context_cap_tokens: 128_000,
        secret_ref: Some(A_KEY),
        platform_default_available: true,
    })
    .expect("the response serializes");

    // Field NAMES, not substrings: `context_cap_tokens` legitimately contains
    // "token", and an assertion that cannot tell those apart fails on correct
    // output — which is how a guard rail gets deleted rather than fixed.
    for forbidden in [
        r#""api_key""#,
        r#""apiKey""#,
        r#""secret_value""#,
        r#""key_material""#,
    ] {
        assert!(
            !rendered.contains(forbidden),
            "the read surface must not carry {forbidden}: {rendered}"
        );
    }
}

#[test]
fn a_write_may_name_a_credential_without_naming_a_model() {
    // A tenant rotating its key keeps the model it already resolved, so the
    // daemon must be able to tell "no model sent" from "model cleared".
    let parsed: TenantProviderRequest =
        serde_json::from_str(r#"{"mode":"self_managed","secret_ref":"openai-primary"}"#)
            .expect("a credential-only write parses");

    assert_eq!(parsed.mode, ProviderMode::SelfManaged);
    assert_eq!(parsed.secret_ref.as_deref(), Some(A_KEY));
    assert_eq!(parsed.model, None);
}

#[test]
fn a_write_naming_only_a_mode_parses_and_is_refused_later() {
    // Deliberately NOT a serde error. A self-managed write with no credential
    // is `UZ-PROVIDER-001` with a sentence a person can act on; a shape error
    // would answer with no registry code at all.
    let parsed: TenantProviderRequest =
        serde_json::from_str(r#"{"mode":"self_managed"}"#).expect("the shape is valid");

    assert_eq!(parsed.secret_ref, None);
}

#[test]
fn a_mode_this_daemon_does_not_serve_is_refused_at_the_boundary() {
    let refused = serde_json::from_str::<TenantProviderRequest>(r#"{"mode":"byo_gateway"}"#);
    assert!(refused.is_err(), "an unknown mode must not parse");
}

#[test]
fn a_stray_field_is_refused_rather_than_ignored() {
    // `deny_unknown_fields`: a client sending `api_key` here is making a
    // mistake worth telling it about, not one to drop on the floor.
    let refused = serde_json::from_str::<TenantProviderRequest>(
        r#"{"mode":"platform","api_key":"sk-live-not-here"}"#,
    );
    assert!(refused.is_err(), "an unknown field must not be ignored");
}
