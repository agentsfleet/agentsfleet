#![expect(
    clippy::unwrap_used,
    reason = "test module: a selection of published names cannot fail"
)]

use serde_json::json;

use super::{NESTED, Nested, RUN_BY_THE_LOOP};
use crate::catalog::{
    DELEGATE, FILE_READ, INTERRUPT_AGENT, LIST_AGENTS, SEND_INPUT, SPAWN, WAIT_AGENT,
};
use crate::runtime::ToolErrorCode;
use crate::testing::{call, hosted, offered};
use crate::{Lease, parsed};

/// Each of the six is named by its entry, and a sandbox tool is none.
#[test]
fn the_six_are_nested_and_a_sandbox_tool_is_not() {
    for entry in NESTED {
        let nested = Nested::of(entry.name());
        assert_eq!(nested.map(Nested::entry), Some(entry), "{}", entry.name());
    }
    assert_eq!(Nested::of(FILE_READ.name()), None);
}

/// A call that reaches a handler, which only the router could make, reads a
/// refusal naming the loop and the tool rather than running anything: each of
/// the six, with arguments its schema admits, so the call reaches its `run`.
#[tokio::test]
async fn a_handler_refuses_what_only_the_loop_runs() {
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = NESTED.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let lease = Lease::default();
    let calls = [
        (&DELEGATE, json!({"task": "read a.md"})),
        (&SPAWN, json!({"task": "read b.md", "tools": ["file_read"]})),
        (&WAIT_AGENT, json!({"child_id": 1, "timeout_ms": 0})),
        (&SEND_INPUT, json!({"child_id": 2, "message": "stop"})),
        (&LIST_AGENTS, json!({})),
        (&INTERRUPT_AGENT, json!({"child_id": 3})),
    ];
    assert_eq!(calls.len(), NESTED.len(), "every nested tool is called");

    for (entry, arguments) in calls {
        let name = entry.name();
        let output = call(offered(&selection, entry), &lease, arguments).await;

        assert_eq!(output.error_code, Some(ToolErrorCode::NotOffered), "{name}");
        let refusal = format!("{name} {RUN_BY_THE_LOOP}");
        assert!(output.text.ends_with(&refusal), "{}", output.text);
    }
}

/// The schema the model is told refuses an argument it does not name, so
/// a child's task is the one field and never a model's name for it.
#[test]
fn the_task_schema_refuses_a_field_it_does_not_name() {
    let refused = parsed::<super::Task>(&json!({"task": "x", "model": "other"}));
    let parsed_task = parsed::<super::Task>(&json!({"task": "x", "tools": ["file_read"]}));

    assert_eq!(
        refused.err().and_then(|output| output.error_code),
        Some(ToolErrorCode::InvalidArguments)
    );
    assert_eq!(
        parsed_task.ok().and_then(|task| task.tools).as_deref(),
        Some(&["file_read".to_owned()][..])
    );
    assert_eq!(SPAWN.name(), "spawn");
}
