//! Frozen field-level shapes for the admin catalogues.
#![expect(clippy::expect_used, reason = "tests inspect JSON fixtures")]

use std::borrow::Cow;

use afd_wire::admin::{
    AdminLibrariesResponse, AdminLibraryCreated, AdminLibraryItem, AdminLibraryRequirements,
    AdminModelCreated, AdminModelItem, AdminModelUpdated, AdminModelsResponse, FleetBundleItem,
    FleetBundlesResponse, ModelRates, PlatformKeyDeactivateResponse, PlatformKeyItem,
    PlatformKeySetResponse, PlatformKeysResponse,
};

#[test]
fn admin_catalogue_wire_shapes_are_metadata_only() {
    let models = AdminModelsResponse {
        models: vec![AdminModelItem {
            id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01"),
            provider: Cow::Borrowed("anthropic"),
            model_id: Cow::Borrowed("claude-opus-5"),
            rates: ModelRates {
                context_cap_tokens: 200_000,
                input_nanos_per_mtok: 5,
                cached_input_nanos_per_mtok: 1,
                output_nanos_per_mtok: 25,
            },
        }],
        request_id: Cow::Borrowed("req-models"),
    };
    let libraries = AdminLibrariesResponse {
        entries: vec![AdminLibraryItem {
            id: Cow::Borrowed("github-reviewer"),
            name: Cow::Borrowed("GitHub reviewer"),
            description: Cow::Borrowed("Reviews pull requests"),
            source_repo: Cow::Borrowed("agentsfleet/github-reviewer"),
            source_ref: Cow::Borrowed("main"),
            visibility: Cow::Borrowed("public"),
            content_hash: Some(Cow::Borrowed("abc123")),
            requirements: AdminLibraryRequirements {
                credentials: vec![Cow::Borrowed("github")],
                tools: vec![Cow::Borrowed("http_request")],
                network_hosts: vec![Cow::Borrowed("api.github.com")],
                trigger_present: true,
            },
            required_credentials_reasons: serde_json::json!({"github":"Reviews pull requests"}),
            updated_at: 1_725_000_000_000,
            etag: Cow::Borrowed("\"abc123\""),
        }],
    };

    assert_eq!(
        serde_json::to_value(models).expect("model response serializes"),
        serde_json::json!({"models":[{"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200_000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}],"request_id":"req-models"})
    );
    let library_json = serde_json::to_value(libraries).expect("library response serializes");
    let library = library_json
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .and_then(|libraries| libraries.first())
        .expect("response holds its one library");
    assert_eq!(
        library
            .get("requirements")
            .and_then(|requirements| requirements.get("credentials")),
        Some(&serde_json::json!(["github"]))
    );
    assert_eq!(library.get("etag"), Some(&serde_json::json!("\"abc123\"")));
    assert!(library.get("skill_markdown").is_none());
    assert!(library.get("support_files").is_none());

    let created = AdminLibraryCreated {
        id: Cow::Borrowed("github-reviewer"),
        name: Cow::Borrowed("github-reviewer"),
        visibility: Cow::Borrowed("platform"),
        content_hash: Cow::Borrowed("abc123"),
        requirements: AdminLibraryRequirements {
            credentials: vec![Cow::Borrowed("github")],
            tools: vec![Cow::Borrowed("http_request")],
            network_hosts: vec![Cow::Borrowed("api.github.com")],
            trigger_present: true,
        },
    };
    assert_eq!(
        serde_json::to_value(created).expect("created library serializes"),
        serde_json::json!({"id":"github-reviewer","name":"github-reviewer","visibility":"platform","content_hash":"abc123","requirements":{"credentials":["github"],"tools":["http_request"],"network_hosts":["api.github.com"],"trigger_present":true}})
    );

    let gallery = FleetBundlesResponse {
        items: vec![FleetBundleItem {
            id: Cow::Borrowed("github-reviewer"),
            name: Cow::Borrowed("GitHub reviewer"),
            description: Cow::Borrowed("Reviews pull requests"),
            required_credentials: vec![Cow::Borrowed("github")],
            required_credentials_reasons: serde_json::json!({"github":"Review pull requests"}),
            required_tools: vec![Cow::Borrowed("http_request")],
            network_hosts: vec![Cow::Borrowed("api.github.com")],
        }],
    };
    assert_eq!(
        serde_json::to_value(gallery).expect("gallery serializes"),
        serde_json::json!({"items":[{"id":"github-reviewer","name":"GitHub reviewer","description":"Reviews pull requests","required_credentials":["github"],"required_credentials_reasons":{"github":"Review pull requests"},"required_tools":["http_request"],"network_hosts":["api.github.com"]}]})
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
            updated_at: 1_725_000_000_000,
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
    let model = AdminModelItem {
        id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01"),
        provider: Cow::Borrowed("anthropic"),
        model_id: Cow::Borrowed("claude-opus-5"),
        rates: ModelRates {
            context_cap_tokens: 200_000,
            input_nanos_per_mtok: 5,
            cached_input_nanos_per_mtok: 1,
            output_nanos_per_mtok: 25,
        },
    };
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
        serde_json::json!({"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200_000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25,"request_id":"req-create"})
    );
    assert_eq!(
        serde_json::to_value(updated).expect("update response serializes"),
        serde_json::json!({"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","updated":true,"request_id":"req-update"})
    );
}
