//! Frozen field-level shapes for the admin catalogues.
#![expect(clippy::expect_used, reason = "tests inspect JSON fixtures")]

use std::borrow::Cow;

use afd_api_wire::admin::{
    AdminModelCreated, AdminModelItem, AdminModelUpdated, AdminModelsResponse,
    PlatformKeyDeactivateResponse, PlatformKeyItem, PlatformKeySetResponse, PlatformKeysResponse,
};
use afd_api_wire::catalogue::{CatalogueMetadata, CataloguePrice};

/// The instant every fixture's evidence was verified at.
const VERIFIED: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);

/// One model with its evidence and a single open-ended base tier.
fn item() -> AdminModelItem<'static> {
    AdminModelItem {
        inference: None,
        id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01"),
        provider: Cow::Borrowed("anthropic"),
        model_id: Cow::Borrowed("claude-opus-5"),
        metadata: CatalogueMetadata {
            display_name: Cow::Borrowed("Opus 5"),
            context_cap_tokens: 200_000,
            context_source_url: Cow::Borrowed("https://example.test/models"),
            context_verified_at: VERIFIED,
            advanced: false,
        },
        prices: vec![CataloguePrice {
            effective_from: VERIFIED,
            effective_until: None,
            prompt_tokens_above: 0,
            input_nanos_per_mtok: 5,
            cached_input_nanos_per_mtok: 1,
            cache_write_nanos_per_mtok: None,
            cache_write_5m_nanos_per_mtok: Some(6),
            cache_write_1h_nanos_per_mtok: None,
            output_nanos_per_mtok: 25,
            source_url: Cow::Borrowed("https://example.test/pricing"),
            verified_at: VERIFIED,
            conditions: Cow::Borrowed(""),
        }],
    }
}

/// [`item`] as the wire spells it, before any envelope field.
fn item_json() -> serde_json::Value {
    serde_json::json!({"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","provider":"anthropic","model_id":"claude-opus-5",
        "metadata":{"display_name":"Opus 5","context_cap_tokens":200_000,"context_source_url":"https://example.test/models",
            "context_verified_at":"2025-10-09T08:53:20Z","advanced":false},
        "prices":[{"effective_from":"2025-10-09T08:53:20Z","effective_until":null,"prompt_tokens_above":0,
            "input_nanos_per_mtok":"5","cached_input_nanos_per_mtok":"1","cache_write_nanos_per_mtok":null,
            "cache_write_5m_nanos_per_mtok":"6","cache_write_1h_nanos_per_mtok":null,"output_nanos_per_mtok":"25",
            "source_url":"https://example.test/pricing","verified_at":"2025-10-09T08:53:20Z","conditions":""}]})
}

#[test]
fn admin_catalogue_wire_shapes_are_metadata_only() {
    let models = AdminModelsResponse {
        models: vec![item()],
        request_id: Cow::Borrowed("req-models"),
    };
    assert_eq!(
        serde_json::to_value(models).expect("model response serializes"),
        serde_json::json!({"models":[item_json()],"request_id":"req-models"})
    );
}

#[test]
fn test_platform_key_success_shapes_never_reveal_key_material() {
    let keys = PlatformKeysResponse {
        keys: vec![PlatformKeyItem {
            provider: Cow::Borrowed("anthropic"),
            source_workspace_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02"),
            model: Some(Cow::Borrowed("claude-opus-5")),
            active: true,
            updated_at: jiff::Timestamp::constant(1_725_000_000, 0),
        }],
        request_id: Cow::Borrowed("req-keys"),
    };
    let set = PlatformKeySetResponse {
        provider: Cow::Borrowed("anthropic"),
        model: Cow::Borrowed("claude-opus-5"),
        source_workspace_id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02"),
        active: true,
        request_id: Cow::Borrowed("req-set"),
    };
    let deactivated = PlatformKeyDeactivateResponse {
        provider: Cow::Borrowed("anthropic"),
        active: false,
        request_id: Cow::Borrowed("req-delete"),
    };

    let json = serde_json::to_value(keys).expect("key response serializes");
    assert_eq!(json.get("request_id"), Some(&serde_json::json!("req-keys")));
    let key = json
        .get("keys")
        .and_then(serde_json::Value::as_array)
        .and_then(|keys| keys.first())
        .expect("response holds its one key");
    assert_eq!(key.get("model"), Some(&serde_json::json!("claude-opus-5")));
    assert!(key.get("api_key").is_none());
    assert!(key.get("ciphertext").is_none());
    assert_eq!(
        serde_json::to_value(set).expect("set response serializes"),
        serde_json::json!({"provider":"anthropic","model":"claude-opus-5","source_workspace_id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02","active":true,"request_id":"req-set"})
    );
    assert_eq!(
        serde_json::to_value(deactivated).expect("delete response serializes"),
        serde_json::json!({"provider":"anthropic","active":false,"request_id":"req-delete"})
    );
}

#[test]
fn test_model_mutation_success_shapes() {
    let model = item();
    let created = AdminModelCreated {
        model,
        request_id: Cow::Borrowed("req-create"),
    };
    let updated = AdminModelUpdated {
        id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01"),
        updated: true,
        request_id: Cow::Borrowed("req-update"),
    };

    assert_eq!(
        serde_json::to_value(created).expect("create response serializes"),
        {
            let mut expected = item_json();
            expected
                .as_object_mut()
                .expect("an item is an object")
                .insert("request_id".into(), serde_json::json!("req-create"));
            expected
        }
    );
    assert_eq!(
        serde_json::to_value(updated).expect("update response serializes"),
        serde_json::json!({"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","updated":true,"request_id":"req-update"})
    );
}
