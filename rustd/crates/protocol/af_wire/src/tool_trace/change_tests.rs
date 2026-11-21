//! An edit's changes on a trace row: every one carried, however many, and a
//! change naming no file refuses the trace.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking, and encodes the trace it built"
)]

use std::borrow::Cow;

use serde_json::Map;

use super::{FileChange, changes_fit};
use crate::tool_trace::{ToolCallStatus, ToolTrace, ToolTraceCall, TraceRejection};

/// A change to `path` of one line each way.
fn change(path: &str) -> FileChange<'static> {
    FileChange {
        path: Cow::Owned(path.to_owned()),
        added: 1,
        removed: 1,
    }
}

/// Call `call_id`, an edit that succeeded with no output.
fn call(call_id: &str) -> ToolTraceCall<'static> {
    ToolTraceCall {
        call_id: Cow::Owned(call_id.to_owned()),
        name: Cow::Borrowed("apply_patch"),
        arguments: Map::new(),
        status: ToolCallStatus::Succeeded,
        output_head: None,
        output_tail: None,
        output_line_count: None,
        exit_code: None,
        duration_ms: 12,
        changes: Vec::new(),
    }
}

fn trace(calls: Vec<ToolTraceCall<'static>>) -> ToolTrace<'static> {
    ToolTrace {
        calls,
        omitted_call_count: 0,
    }
}

#[test]
fn an_edit_carries_every_change_it_made() {
    let mut edited = call("1");
    edited.changes = (1..=300).map(|file| change(&format!("f{file}"))).collect();
    assert_eq!(trace(vec![edited.clone()]).validate(), Ok(()));
    assert!(changes_fit(&edited.changes));
    let text = serde_json::to_string(&trace(vec![edited])).expect("encodes");
    assert!(text.contains(r#""changes":[{"path":"f1","added":1,"removed":1}"#));
    let plain = serde_json::to_string(&trace(vec![call("1")])).expect("encodes");
    assert!(
        !plain.contains("changes"),
        "a call that changed nothing carries no changes field"
    );
}

#[test]
fn a_change_naming_no_file_refuses_the_trace() {
    let mut empty = call("1");
    empty.changes = vec![change("")];
    assert!(!changes_fit(&empty.changes));
    assert_eq!(
        trace(vec![empty]).validate(),
        Err(TraceRejection::Malformed)
    );
    let mut long = call("1");
    long.changes = vec![change(&"p".repeat(4096))];
    assert!(changes_fit(&long.changes), "no length caps a path");
    assert_eq!(trace(vec![long]).validate(), Ok(()));
    assert!(!changes_fit(&[change("a\u{0}")]), "a NUL never fits");
}

#[test]
fn a_nul_in_a_changed_path_drops_the_trace() {
    let mut changed = call("1");
    changed.changes = vec![change("src/\u{0}a.rs")];
    assert_eq!(
        trace(vec![changed]).validate(),
        Err(TraceRejection::HoldsNul)
    );
}
