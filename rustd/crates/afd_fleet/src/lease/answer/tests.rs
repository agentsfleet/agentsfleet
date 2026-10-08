//! What an issued lease tells its runner about the sandbox the runner holds.
//!
//! The claim decides whether the holder resumes its hold; this proves the
//! decision reaches the bytes the runner reads, in both directions, so a
//! render that dropped it or spelled it constant fails here.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::borrow::Cow;

use afd_fleet_runtime::FleetConfig;
use afd_wire::event::EventType;
use afd_wire::policy::{ContextBudget, ExecutionPolicy, NetworkPolicy};
use serde_json::Value;

use super::render;
use crate::lease::envelope::Acquired;
use crate::lease::installed::{FRESH_CONTEXT, Installed};
use crate::lease::test_dead;

/// Where the rendered answer says whether to resume the held sandbox.
const RESUME_HOLD: &str = "/lease/resume_hold";

/// The fixture fleet's name, in its document and its row alike.
const FLEET_NAME: &str = "fixture";

/// The smallest fleet document a stored config accepts.
fn document() -> String {
    serde_json::json!({
        "name": FLEET_NAME,
        "x-agentsfleet": {"triggers": [{"type": "api"}], "tools": [], "budget": {"daily_dollars": 1}},
    })
    .to_string()
}

/// The fleet as the claim read it, built from no bundle.
fn installed() -> Installed {
    Installed {
        workspace_id: test_dead::id(2),
        name: FLEET_NAME.to_owned(),
        config: FleetConfig::authored(&document()).expect("the fixture document is authorable"),
        instructions: String::new(),
        context_json: FRESH_CONTEXT.to_owned(),
        bundle_content_hash: None,
    }
}

/// A policy granting nothing, which the render copies through untouched.
fn policy() -> ExecutionPolicy<'static> {
    ExecutionPolicy {
        network_policy: NetworkPolicy {
            allow: Vec::new(),
            read_only: true,
            read_post_paths: Vec::new(),
        },
        tools: Vec::new(),
        secrets_map: None,
        mintable: Vec::new(),
        provider: Cow::Borrowed("anthropic"),
        api_key: Cow::Borrowed("fixture-key"),
        inference_host: Cow::Borrowed("api.example"),
        base_url: None,
        repository_binding: None,
        http_origin_policies: Vec::new(),
        context: ContextBudget {
            tool_window: 20,
            memory_checkpoint_every: 5,
            stage_chunk_threshold: 0.75,
            model: Cow::Borrowed("model"),
            context_cap_tokens: 0,
        },
    }
}

/// The answer a runner reads for a claim that did, or did not, find its hold
/// live.
fn rendered(resume_hold: bool) -> Value {
    let acquired = Acquired {
        resume_hold,
        ..test_dead::acquired()
    };
    let installed = installed();
    let answer = render(
        &test_dead::id(4),
        &acquired,
        EventType::Chat,
        &installed,
        policy(),
        Vec::new(),
    )
    .expect("a lease renders");
    serde_json::from_str(&answer).expect("the answer is JSON")
}

#[test]
fn test_a_rendered_lease_says_whether_the_claim_resumes_the_hold() {
    for resume_hold in [true, false] {
        assert_eq!(
            rendered(resume_hold).pointer(RESUME_HOLD),
            Some(&Value::Bool(resume_hold)),
            "the claim's resume_hold is {resume_hold}"
        );
    }
}
