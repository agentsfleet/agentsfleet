use afd_wire::policy::ExecutionPolicy;
use afr_providers::Message;

use super::{Budget, EVICTED};
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

#[test]
fn should_reach_the_cap_at_its_token_count_and_never_under_a_zero_cap() {
    assert!(!budget_of(0, 50).reached(49));
    assert!(budget_of(0, 50).reached(50));
    assert!(!budget_of(0, 0).reached(u64::MAX));
}
