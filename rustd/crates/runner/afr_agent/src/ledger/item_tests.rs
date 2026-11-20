//! What each call's item says of it: the turn newest when its step began,
//! what kind of call it was, and, for an edit, the files it changed. Split
//! from `tests.rs` at the length cap.
#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::activity::ActivityFrame;
use af_wire::tool_detail::{ItemKind, ItemStatus};
use af_wire::tool_trace::FileChange;
use afr_tools::catalog::UPDATE_PLAN;
use guffbolt::tools::files::changes::{Change, report};
use guffbolt::{Call, Mark, ToolOutput};

use super::Ledger;
use super::test_calls::{RUN_ID, TURN, call, marked, planned, ran};
use crate::fixture::{Frames, scrub};

/// A turn a person sent while the run worked.
const SECOND: &str = "019f0000-0000-7000-8000-000000000002";

/// A call's item names the turn its step began with; one whose step carried
/// no mark, such as a child's, names the newest the run heard, and the run's
/// own turn before any.
#[test]
fn each_record_names_the_turn_its_step_began_with() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let second = Mark::new(SECOND);

    call(&ledger, "before any message was folded".to_owned());
    let folded = planned();
    let answered = Some(ToolOutput::succeeded("after the second message"));
    marked(
        &ledger,
        &folded,
        ledger.open(&folded),
        answered,
        Some(&second),
    );
    call(&ledger, "a child's call, unmarked".to_owned());
    let (_trace, records) = ledger.finish();
    frames.taken();

    let turns: Vec<&str> = records
        .iter()
        .map(|record| record.turn_id.as_ref())
        .collect();
    assert_eq!(turns, [TURN, SECOND, SECOND]);
}

/// The report `apply_patch` ends with: `src/a.rs` by +24 −8.
fn patched() -> String {
    let change = Change {
        path: "src/a.rs".to_owned(),
        added: 24,
        removed: 8,
    };
    report(
        "Success. Updated the following files:\nM src/a.rs",
        &[change],
    )
}

fn patch() -> Call {
    Call {
        name: "apply_patch".to_owned(),
        ..planned()
    }
}

#[test]
fn an_edit_carries_its_changes_on_its_record_its_trace_row_and_its_end_frame() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    ran(&ledger, &patch(), Some(ToolOutput::succeeded(patched())));
    let (trace, records) = ledger.finish();

    let expected = vec![FileChange {
        path: "src/a.rs".into(),
        added: 24,
        removed: 8,
    }];
    assert_eq!(records.len(), 1, "one edit item");
    let item = &records[0];
    assert_eq!(
        (item.kind, item.tool.as_ref(), item.status),
        (ItemKind::Edit, "apply_patch", ItemStatus::Succeeded)
    );
    assert_eq!(item.changes, expected);
    assert_eq!(trace.calls[0].changes, expected);
    let ended: Vec<Vec<FileChange<'_>>> = frames
        .taken()
        .into_iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallCompleted(done) => Some(done.changes),
            _ => None,
        })
        .collect();
    assert_eq!(ended, [expected]);
}

#[test]
fn a_failed_edit_is_an_edit_item_with_no_changes() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);
    let exited = ToolOutput {
        exit_code: Some(1),
        ..ToolOutput::succeeded(patched())
    };

    ran(&ledger, &patch(), Some(exited));
    let (trace, records) = ledger.finish();
    frames.taken();

    assert_eq!(records.len(), 1);
    assert_eq!(
        (records[0].kind, records[0].status),
        (ItemKind::Edit, ItemStatus::Failed)
    );
    assert!(
        records[0].changes.is_empty(),
        "a failed edit counts nothing"
    );
    assert_eq!(trace.calls[0].changes, Vec::new());
}

#[test]
fn a_call_that_is_not_an_edit_is_a_tool_call_item() {
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    // Output shaped like a report does not make a plan update an edit.
    call(&ledger, patched());
    let (_trace, records) = ledger.finish();
    frames.taken();

    assert_eq!(records[0].kind, ItemKind::ToolCall);
    assert_eq!(records[0].tool, UPDATE_PLAN.name());
    assert_eq!(records[0].changes, Vec::new());
}
