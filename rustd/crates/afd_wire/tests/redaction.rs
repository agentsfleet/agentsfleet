//! Secrets must not reach a log through `Debug`, and must still reach the wire.
//!
//! `M-PUBLIC-DEBUG` requires a type holding sensitive data to implement `Debug`
//! by hand AND to carry tests proving the data does not leak — "so it isn't, and
//! will not be in future". These are those tests: each asserts the secret is
//! absent from the rendered form and present in the serialized form, so a future
//! edit cannot fix one by breaking the other.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a serialization failure should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::admin::RunnerTokenRotatedResponse;
use afd_wire::credentials::MintCredentialResponse;
use afd_wire::policy::{ContextBudget, ExecutionPolicy, NetworkPolicy};
use afd_wire::runner::{
    AssignedPolicy, NetworkPolicy as EgressPolicy, RegisterResponse, SandboxTier,
};

/// A value no legitimate field would contain, so finding it anywhere in a
/// rendered string is unambiguous evidence of a leak.
const SECRET: &str = "sk-live-CANARY-must-never-appear-in-a-log";

fn policy() -> ExecutionPolicy<'static> {
    ExecutionPolicy {
        network_policy: NetworkPolicy {
            allow: vec![],
            read_only: true,
            read_post_paths: vec![],
        },
        tools: vec![],
        secrets_map: Some(serde_json::json!({ "github": { "token": SECRET } })),
        mintable: vec![],
        provider: Cow::Borrowed("anthropic"),
        api_key: Cow::Borrowed(SECRET),
        inference_host: Cow::Borrowed("api.example"),
        base_url: None,
        repository_binding: None,
        http_origin_policies: vec![],
        context: ContextBudget {
            tool_window: 20,
            memory_checkpoint_every: 5,
            stage_chunk_threshold: 0.75,
            model: Cow::Borrowed("model"),
            context_cap_tokens: 0,
        },
    }
}

#[test]
fn should_not_leak_the_provider_key_or_secrets_map_through_debug() {
    let rendered = format!("{:?}", policy());
    assert!(
        !rendered.contains(SECRET),
        "provider key leaked: {rendered}"
    );
    assert!(
        !rendered.contains("github"),
        "a secrets_map KEY leaked, which names a connected integration: {rendered}"
    );
    assert!(rendered.contains("<redacted>"), "{rendered}");
    // The non-secret fields must still be readable, or redaction has made the
    // type useless to debug with and someone will delete it.
    assert!(rendered.contains("anthropic"), "{rendered}");
    assert!(rendered.contains("api.example"), "{rendered}");
}

/// The other half: redaction is a `Debug` concern only. These types exist to put
/// the secret on the wire, so a redacted SERIALIZATION would be a silent outage.
#[test]
fn should_still_serialize_the_real_secret_to_the_wire() {
    let json = serde_json::to_string(&policy()).unwrap();
    assert!(json.contains(SECRET), "the wire must carry the real value");
    assert!(
        !json.contains("<redacted>"),
        "redaction leaked into serialization"
    );
}

#[test]
fn should_not_leak_the_runner_token_through_debug() {
    let response = RegisterResponse {
        runner_id: Cow::Borrowed("runner"),
        runner_token: Cow::Borrowed(SECRET),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: EgressPolicy::AllowAll,
            registry_allowlist: vec![],
            worker_count: 1,
            extra_binds: vec![],
        },
    };
    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains(SECRET),
        "runner token leaked: {rendered}"
    );
    assert!(
        rendered.contains("runner"),
        "the identifier must stay readable"
    );
    assert!(serde_json::to_string(&response).unwrap().contains(SECRET));
}

#[test]
fn should_not_leak_the_rotated_runner_token_through_debug() {
    let response = RunnerTokenRotatedResponse {
        id: Cow::Borrowed("runner"),
        runner_token: Cow::Borrowed(SECRET),
    };
    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains(SECRET),
        "runner token leaked: {rendered}"
    );
    assert!(
        rendered.contains("runner"),
        "the identifier must stay readable"
    );
    assert!(serde_json::to_string(&response).unwrap().contains(SECRET));
}

#[test]
fn should_not_leak_a_minted_credential_through_debug() {
    let response = MintCredentialResponse {
        token: Cow::Borrowed(SECRET),
        expires_at_ms: 42,
    };
    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains(SECRET),
        "minted token leaked: {rendered}"
    );
    assert!(rendered.contains("42"), "the expiry must stay readable");
    assert!(serde_json::to_string(&response).unwrap().contains(SECRET));
}

/// A lease embeds the policy, so a derived `Debug` anywhere up the tree would
/// re-expose what the leaf redacts. This is the assertion that catches it.
#[test]
fn should_not_leak_a_secret_through_an_enclosing_type() {
    let json = serde_json::to_string(&policy()).unwrap();
    let wrapper = format!("{:?}", vec![policy()]);
    assert!(
        !wrapper.contains(SECRET),
        "leaked through a container: {wrapper}"
    );
    assert!(json.contains(SECRET));
}

/// A `secrets_map` that is not a JSON object still redacts. The field is
/// free-form by type, so a peer sending a string or an array must not slip the
/// value through the branch that counts object entries.
#[test]
fn should_redact_a_secrets_map_that_is_not_an_object() {
    for value in [
        serde_json::Value::String(SECRET.to_owned()),
        serde_json::json!([SECRET]),
        serde_json::Value::Bool(true),
    ] {
        let mut policy = policy();
        policy.secrets_map = Some(value);
        let rendered = format!("{policy:?}");
        assert!(!rendered.contains(SECRET), "leaked: {rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }
}
