use afd_wire::policy::ExecutionPolicy;
use afr_providers::Message;

use super::{Budget, Checkpoints, EVICTED};
use crate::fixture::{budget, lease};

fn budget_of(window: u32, cap: u32) -> Budget {
    let lease = lease(&[], budget(window, cap));
    let policy: &ExecutionPolicy<'_> = &lease.policy;
    Budget::new(&policy.context)
}

fn result(output: &str) -> Message {
    Message::ToolResult {
        call_id: "c".to_owned(),
        output: output.to_owned(),
    }
}

fn outputs(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.clone()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect()
}

#[test]
fn should_keep_only_the_newest_results_in_the_window() {
    let mut messages = vec![
        Message::User("ask".to_owned()),
        result("1"),
        result("2"),
        result("3"),
    ];

    budget_of(2, 0).evict(&mut messages);

    assert_eq!(outputs(&messages), [EVICTED, "2", "3"]);
    assert_eq!(messages.first(), Some(&Message::User("ask".to_owned())));
}

#[test]
fn should_keep_every_result_under_a_zero_window() {
    let mut messages = vec![result("1"), result("2")];

    budget_of(0, 0).evict(&mut messages);

    assert_eq!(outputs(&messages), ["1", "2"]);
}

// The fixture's stage fraction is 0.75: a 100-token window fills at 75.
#[test]
fn should_reach_the_cap_at_the_stage_fraction_and_never_under_a_zero_cap() {
    assert!(!budget_of(0, 100).reached(74));
    assert!(budget_of(0, 100).reached(75));
    assert!(budget_of(0, 100).reached(u64::MAX));
    assert!(!budget_of(0, 0).reached(u64::MAX));
}

#[test]
fn should_never_checkpoint_at_a_zero_cadence_and_every_n_calls_otherwise() {
    let cadence = |every: u32| {
        let lease = lease(
            &[],
            serde_json::json!({"tool_window": 0, "memory_checkpoint_every": every,
                "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0}),
        );
        let mut checkpoints = Checkpoints::new(&lease.policy.context);
        (0..6).map(|_call| checkpoints.due()).collect::<Vec<_>>()
    };

    assert_eq!(cadence(0), [false; 6]);
    assert_eq!(cadence(1), [true; 6]);
    assert_eq!(cadence(3), [false, false, true, false, false, true]);
}
