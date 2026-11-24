//! What an issued run tells its runner about the sandbox the runner holds.
//!
//! The assignment decides whether the holder resumes its hold; this proves the
//! decision reaches the bytes the runner reads, in both directions, so a
//! render that dropped it or spelled it constant fails here.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::borrow::Cow;

use af_wire::policy::{ContextBudget, ExecutionPolicy, NetworkPolicy};
use af_wire::turn::TurnKind;
use serde_json::Value;

use super::{Context, payload};
use crate::run::envelope::Acquired;
use crate::run::installed::{FRESH_CONTEXT, Installed};
use crate::run::test_dead;

/// Where the rendered answer says whether to resume the held sandbox.
const RESUME_HOLD: &str = "/run/resume_hold";

/// Where the rendered answer names the fleet to its runner.
const NAME: &str = "/run/name";

/// The fixture fleet's name.
const FLEET_NAME: &str = "Support triage";

/// The fleet as the assignment read it from its row.
fn installed() -> Installed {
    Installed {
        workspace_id: test_dead::id(2),
        name: FLEET_NAME.to_owned(),
        model_ref: "platform-default".to_owned(),
        http_egress: af_fleet_runtime::config::HttpEgress::Restricted,
        package_domains: false,
        context_json: FRESH_CONTEXT.to_owned(),
    }
}

/// A policy granting nothing, which the render copies through untouched.
fn policy() -> ExecutionPolicy<'static> {
    ExecutionPolicy {
        inference: None,
        model_context_tokens: 0,
        network_policy: NetworkPolicy::default(),
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

/// The answer a runner reads for an assignment that did, or did not, find its hold
/// live.
fn rendered(resume_hold: bool) -> Value {
    let acquired = Acquired {
        predecessor_run_id: None,
        resume_hold,
        ..test_dead::acquired()
    };
    let installed = installed();
    let run_id = test_dead::id(4);
    let run = payload(
        &run_id,
        &acquired,
        TurnKind::Chat,
        &installed,
        policy(),
        Context {
            turn_resume: None,
            history: Vec::new(),
            finished_calls: Vec::new(),
            memory: None,
            decision: None,
            task: None,
            expires_at: acquired.held_until,
            unfinished_tasks: Vec::new(),
        },
    );
    serde_json::to_value(af_wire::protocol::Down::Run {
        run: af_wire::run::TaggedRun::Fleet(Box::new(run)),
    })
    .expect("an assignment renders")
}

#[test]
fn a_rendered_run_names_its_fleet() {
    assert_eq!(
        rendered(false).pointer(NAME),
        Some(&Value::String(FLEET_NAME.to_owned()))
    );
}

#[test]
fn test_a_rendered_run_says_whether_the_assignment_resumes_the_hold() {
    for resume_hold in [true, false] {
        assert_eq!(
            rendered(resume_hold).pointer(RESUME_HOLD),
            Some(&Value::Bool(resume_hold)),
            "the assignment's resume_hold is {resume_hold}"
        );
    }
}
