//! Masking and stream-boundary behavior through the public secret API.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

mod masking;
mod streaming;

use std::borrow::Cow;

use afd_wire::policy::ExecutionPolicy;

use afr_secrets::{Carry, Scrub};

/// The provider key the fixture policy carries.
const API_KEY: &str = "sk-test-0123456789";
/// The static credential the fixture policy carries.
const GITHUB_TOKEN: &str = "ghp_fixture_token_abcdef";

/// The scrub of a policy carrying the fixture secrets.
fn scrub() -> Scrub {
    let policy = serde_json::json!({
        "network_policy": {"allow": [], "read_only": true, "read_post_paths": []},
        "tools": [],
        "secrets_map": {"github": {"token": GITHUB_TOKEN, "host": "api.github.com"}},
        "mintable": [], "provider": "anthropic", "api_key": API_KEY,
        "inference_host": "h", "base_url": null, "repository_binding": null,
        "http_origin_policies": [],
        "context": {"tool_window": 0, "memory_checkpoint_every": 0,
            "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0}
    });
    let text = policy.to_string();
    let policy: ExecutionPolicy<'_> = serde_json::from_str(&text).unwrap();
    Scrub::new(&policy).unwrap()
}
