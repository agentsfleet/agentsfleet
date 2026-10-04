#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::lease::LeasePayload;

use super::{Connect as _, Connector};
use crate::registry::Registry;

/// A key no rendering may show.
const KEY: &str = "sk-never-printed";

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
fn should_admit_what_it_can_connect_and_nothing_else() {
    let connector = Connector::new(Registry::builtin().unwrap()).unwrap();

    connector.admit(&lease("anthropic").policy).unwrap();
    connector.admit(&lease("groq").policy).unwrap();
    let refused = connector.admit(&lease("bedrock").policy).unwrap_err();
    let unconnected = connector.connect(&lease("bedrock")).unwrap_err();
    assert_eq!(refused.unhosted_provider(), Some("bedrock"));
    assert_eq!(unconnected.unhosted_provider(), Some("bedrock"));
}

// Where each wire posts is proven on a socket in `tests/providers.rs`; here,
// that a connected provider names its wire and never its key.
#[test]
fn should_connect_each_wire_and_never_print_the_key() {
    let connector = Connector::new(Registry::builtin().unwrap()).unwrap();

    let messages = format!("{:?}", connector.connect(&lease("anthropic")).unwrap());
    let chat = format!("{:?}", connector.connect(&lease("groq")).unwrap());

    assert!(messages.contains("Messages"), "{messages}");
    assert!(chat.contains("Chat"), "{chat}");
    assert!(!messages.contains(KEY) && !chat.contains(KEY));
}
