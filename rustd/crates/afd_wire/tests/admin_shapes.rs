//! Frozen field-level shapes for the admin catalogues.
#![expect(clippy::expect_used, reason = "tests inspect JSON fixtures")]

use std::borrow::Cow;

use afd_wire::admin::{
    AdminLibrariesResponse, AdminLibraryItem, AdminModelItem, AdminModelsResponse, ModelRates,
};

#[test]
fn test_admin_crud_shape_parity() {
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
    };
    let libraries = AdminLibrariesResponse {
        libraries: vec![AdminLibraryItem {
            id: Cow::Borrowed("github-reviewer"),
            name: Cow::Borrowed("GitHub reviewer"),
            description: Cow::Borrowed("Reviews pull requests"),
            source_repo: Cow::Borrowed("agentsfleet/github-reviewer"),
            source_ref: Cow::Borrowed("main"),
            visibility: Cow::Borrowed("public"),
            content_hash: Some(Cow::Borrowed("abc123")),
            required_credentials: serde_json::json!(["github"]),
            required_tools: serde_json::json!(["http_request"]),
            network_hosts: serde_json::json!(["api.github.com"]),
            trigger_present: true,
            updated_at: 1_725_000_000_000,
        }],
    };

    assert_eq!(
        serde_json::to_value(models).expect("model response serializes"),
        serde_json::json!({"models":[{"id":"0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d01","provider":"anthropic","model_id":"claude-opus-5","context_cap_tokens":200000,"input_nanos_per_mtok":5,"cached_input_nanos_per_mtok":1,"output_nanos_per_mtok":25}]})
    );
    let library_json = serde_json::to_value(libraries).expect("library response serializes");
    assert_eq!(library_json["libraries"][0]["required_credentials"], serde_json::json!(["github"]));
    assert!(library_json["libraries"][0].get("skill_markdown").is_none());
    assert!(library_json["libraries"][0].get("support_files").is_none());
}
