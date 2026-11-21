//! An edit's changes come from its tool's report, and from nothing else.

use af_core::test_util::trace::Capture;
use af_wire::tool_detail::{ItemKind, ItemStatus};
use af_wire::tool_trace::{FileChange, ToolCallStatus};
use guffbolt::tools::files::changes::{Change, report};

use super::{EVENT_CHANGES_DROPPED, changed, item_status, kind};

const RUN_ID: &str = "run-1";

/// What a file tool answers after changing `changes`.
fn answered(changes: &[Change]) -> String {
    report("Success. Updated the following files:", changes)
}

fn change(path: &str, added: u64, removed: u64) -> Change {
    Change {
        path: path.to_owned(),
        added,
        removed,
    }
}

#[test]
fn a_succeeded_edit_carries_the_files_its_report_names() {
    let output = answered(&[change("src/a.rs", 24, 8), change("README.md", 1, 0)]);

    let carried = changed(RUN_ID, ItemKind::Edit, ToolCallStatus::Succeeded, &output);

    assert_eq!(
        carried,
        [
            FileChange {
                path: "src/a.rs".into(),
                added: 24,
                removed: 8
            },
            FileChange {
                path: "README.md".into(),
                added: 1,
                removed: 0
            },
        ]
    );
}

#[test]
fn a_call_that_is_not_an_edit_carries_no_changes_even_beside_a_report() {
    // A shell command that printed a report-shaped block is not an edit.
    let output = answered(&[change("src/a.rs", 24, 8)]);

    assert_eq!(
        changed(
            RUN_ID,
            ItemKind::ToolCall,
            ToolCallStatus::Succeeded,
            &output
        ),
        Vec::new()
    );
}

#[test]
fn an_edit_that_did_not_succeed_carries_no_changes() {
    let output = answered(&[change("src/a.rs", 24, 8)]);
    for status in [ToolCallStatus::Failed, ToolCallStatus::Interrupted] {
        assert_eq!(
            changed(RUN_ID, ItemKind::Edit, status, &output),
            Vec::new(),
            "{status:?}"
        );
    }
}

#[test]
fn an_edit_without_a_report_carries_no_changes() {
    assert_eq!(
        changed(
            RUN_ID,
            ItemKind::Edit,
            ToolCallStatus::Succeeded,
            "Wrote 4 bytes to a.txt"
        ),
        Vec::new()
    );
}

#[test]
fn a_report_of_a_hundred_changes_is_carried_whole() {
    let many: Vec<Change> = (0..100)
        .map(|file| change(&format!("f{file}.txt"), 1, 0))
        .collect();

    let carried = changed(
        RUN_ID,
        ItemKind::Edit,
        ToolCallStatus::Succeeded,
        &answered(&many),
    );

    assert_eq!(carried.len(), 100);
}

#[test]
fn a_report_naming_no_path_is_dropped_and_logged() {
    let capture = Capture::install();
    let bad = [change("", 1, 0)];

    let carried = changed(
        RUN_ID,
        ItemKind::Edit,
        ToolCallStatus::Succeeded,
        &answered(&bad),
    );

    assert_eq!(carried, Vec::new());
    let dropped: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_CHANGES_DROPPED))
        .collect();
    assert_eq!(dropped.len(), 1);
    assert_eq!(
        dropped.first().and_then(|event| event.field("files")),
        Some("1")
    );
    assert_eq!(
        dropped.first().and_then(|event| event.field("run_id")),
        Some(RUN_ID)
    );
}

#[test]
fn a_report_naming_an_empty_path_is_dropped() {
    let output = answered(&[change("", 1, 0)]);

    assert_eq!(
        changed(RUN_ID, ItemKind::Edit, ToolCallStatus::Succeeded, &output),
        Vec::new()
    );
}

#[test]
fn each_file_tool_that_edits_is_an_edit_and_every_other_tool_a_tool_call() {
    for tool in [
        "file_write",
        "file_append",
        "file_edit",
        "file_edit_hashed",
        "apply_patch",
    ] {
        assert_eq!(kind(tool), ItemKind::Edit, "{tool}");
    }
    for tool in [
        "file_read",
        "file_delete",
        "shell",
        "git",
        "",
        "APPLY_PATCH",
    ] {
        assert_eq!(kind(tool), ItemKind::ToolCall, "{tool}");
    }
}

#[test]
fn a_call_that_answered_keeps_its_status_on_its_item() {
    assert_eq!(
        item_status(ToolCallStatus::Succeeded),
        ItemStatus::Succeeded
    );
    assert_eq!(item_status(ToolCallStatus::Failed), ItemStatus::Failed);
}
