#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::lease::LeasePayload;
use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use reqwest::Url;

use super::{ANTHROPIC, ANTHROPIC_BASE, Connect as _, Connector, Endpoints, OPENAI, Wire};

/// A self-hosted endpoint a `custom:` provider names.
const CUSTOM_BASE: &str = "https://vllm.corp/v1";
/// A key no rendering may show.
const KEY: &str = "sk-never-printed";

fn custom() -> String {
    format!("{CUSTOM_PROVIDER_PREFIX}{CUSTOM_BASE}")
}

/// A lease naming `provider`, with [`KEY`].
fn lease(provider: &str) -> LeasePayload<'static> {
    let document = serde_json::json!({
        "lease_id": "lease-1", "fencing_token": 1, "lease_expires_at": 1,
        "secret_delivery": "inline",
        "event": {"event_id": "e", "fleet_id": "f", "workspace_id": "w", "actor": "a",
            "event_type": "chat", "request_json": "{}", "created_at": 1},
        "policy": {"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": [], "secrets_map": null, "mintable": [], "provider": provider,
            "api_key": KEY, "inference_host": "", "base_url": null, "repository_binding": null,
            "http_origin_policies": [], "context": {"tool_window": 0, "memory_checkpoint_every": 0,
                "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0}},
        "instructions": "", "bundle": null
    });
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).unwrap()
}

#[test]
fn should_pick_each_wire_by_its_provider_name() {
    assert_eq!(Wire::of(ANTHROPIC).unwrap(), Wire::Messages);
    assert_eq!(Wire::of(OPENAI).unwrap(), Wire::Responses);
    assert_eq!(
        Wire::of(&custom()).unwrap(),
        Wire::Chat(Url::parse(CUSTOM_BASE).unwrap())
    );
}

#[test]
fn should_refuse_a_name_no_wire_speaks_and_a_custom_url_that_does_not_parse() {
    let named = Wire::of("groq").unwrap_err();
    let unparsed = Wire::of(&format!("{CUSTOM_PROVIDER_PREFIX}not a url")).unwrap_err();

    assert_eq!(named.unhosted_provider(), Some("groq"));
    assert!(unparsed.unhosted_provider().is_some());
    assert!(
        Wire::of("Anthropic").is_err(),
        "the daemon sends the lower-case name"
    );
}

#[test]
fn should_admit_what_it_can_connect_and_nothing_else() {
    let connector = Connector::new(Endpoints::default()).unwrap();

    connector.admit(&lease(ANTHROPIC).policy).unwrap();
    connector.admit(&lease(&custom()).policy).unwrap();
    let refused = connector.admit(&lease("groq").policy).unwrap_err();
    let unconnected = connector.connect(&lease("groq")).unwrap_err();
    assert_eq!(refused.unhosted_provider(), Some("groq"));
    assert_eq!(unconnected.unhosted_provider(), Some("groq"));
}

#[test]
fn should_dial_each_wire_under_its_base_and_never_print_the_key() {
    let connector = Connector::new(Endpoints::default()).unwrap();

    let messages = format!("{:?}", connector.connect(&lease(ANTHROPIC)).unwrap());
    let chat = format!("{:?}", connector.connect(&lease(&custom())).unwrap());

    assert!(
        messages.contains(&format!("{ANTHROPIC_BASE}/v1/messages")),
        "{messages}"
    );
    assert!(
        chat.contains("https://vllm.corp/v1/chat/completions"),
        "{chat}"
    );
    assert!(!messages.contains(KEY) && !chat.contains(KEY));
}
