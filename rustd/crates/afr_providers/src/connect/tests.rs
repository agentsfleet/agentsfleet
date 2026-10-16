#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::lease::LeasePayload;

use super::{Connect as _, Connector};
use afd_wire::inference::{InferenceApi, ModelInference};

/// A key no rendering may show.
const KEY: &str = "sk-never-printed";

/// A lease naming `provider`, with [`KEY`].
fn lease(provider: &str) -> LeasePayload<'static> {
    let document = serde_json::json!({
        "lease_id": "lease-1", "fencing_token": 1, "lease_expires_at": 1,
        "secret_delivery": "inline",
        "event": {"event_id": "e", "fleet_id": "f", "workspace_id": "w", "actor": "a",
            "event_type": "chat", "request_json": "{}", "created_at": 1},
        "policy": {"model_context_tokens": 200_000,"network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
            "tools": [], "secrets_map": null, "mintable": [], "provider": provider,
            "api_key": KEY, "inference_host": "", "base_url": null, "repository_binding": null,
            "http_origin_policies": [], "context": {"tool_window": 0, "memory_checkpoint_every": 0,
                "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 200_000}},
        "instructions": "", "limits": null, "history": [], "resume_hold": false
    });
    let text: &'static str = Box::leak(document.to_string().into_boxed_str());
    serde_json::from_str(text).unwrap()
}

fn configured(api: InferenceApi) -> LeasePayload<'static> {
    let mut lease = lease("workspace-provider");
    lease.policy.inference = Some(ModelInference {
        api,
        base_url: "https://models.example.com/v1".into(),
        dialect: None,
        max_output_tokens: 1234,
    });
    lease
}

#[test]
fn should_refuse_missing_or_invalid_settings_before_connecting() {
    let connector = Connector::new().unwrap();
    let absent = lease("unconfigured");
    assert_eq!(
        connector
            .admit(&absent.policy)
            .unwrap_err()
            .unhosted_provider(),
        Some("unconfigured")
    );
    assert_eq!(
        connector.connect(&absent).unwrap_err().unhosted_provider(),
        Some("unconfigured")
    );
    for base in [
        "http://models.example.com",
        "https://127.0.0.1",
        "https://[::1]",
    ] {
        let mut lease = configured(InferenceApi::Chat);
        lease.policy.inference.as_mut().unwrap().base_url = base.into();
        assert!(connector.admit(&lease.policy).is_err());
        assert_eq!(
            connector.connect(&lease).unwrap_err().code(),
            afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
        );
    }
}

#[test]
fn should_connect_every_explicit_api_and_never_print_the_key() {
    let connector = Connector::new().unwrap();
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
    ] {
        let lease = configured(api);
        connector.admit(&lease.policy).unwrap();
        let rendered = format!("{:?}", connector.connect(&lease).unwrap());
        assert!(rendered.contains(&format!("{api:?}")), "{rendered}");
        assert!(!rendered.contains(KEY), "{rendered}");
    }
}

#[test]
fn should_reject_unknown_dialects_and_output_limits_above_context() {
    let connector = Connector::new().unwrap();
    let mut lease = configured(InferenceApi::Messages);
    lease.policy.inference.as_mut().unwrap().dialect = Some("unknown".into());
    assert!(connector.admit(&lease.policy).is_err());
    lease.policy.inference.as_mut().unwrap().dialect = None;
    lease.policy.inference.as_mut().unwrap().max_output_tokens = 200_001;
    assert_eq!(
        connector.connect(&lease).unwrap_err().code(),
        afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
}
