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
    let mut exited = ToolOutput::succeeded(patched());
    exited.exit_code = Some(1);

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

/// A tool environment rooted in a scratch directory holding `src/a.rs`: one
/// kept line, eight lines an edit replaces, and one more kept line.
struct Desk {
    directory: tempfile::TempDir,
    env: guffbolt::tools::LocalEnv,
}

/// The eight lines the edit takes out.
fn removed() -> String {
    let mut text = String::new();
    for line in 1..=8 {
        text.push_str("old line ");
        text.push_str(&line.to_string());
        text.push('\n');
    }
    text
}

/// The twenty-four lines it puts in their place.
fn added() -> String {
    let mut text = String::new();
    for line in 1..=24 {
        text.push_str("new line ");
        text.push_str(&line.to_string());
        text.push('\n');
    }
    text
}

impl Desk {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        std::fs::create_dir_all(directory.path().join("src"))?;
        let text = format!("fn keep() {{}}\n{}fn tail() {{}}\n", removed());
        std::fs::write(directory.path().join("src/a.rs"), text)?;
        let env = guffbolt::tools::LocalEnv::new(directory.path())?;
        Ok(Self { directory, env })
    }

    /// Runs `edit` with the real `file_edit` tool, as a loop dispatches it.
    async fn edit(&self, edit: &Call) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let config = guffbolt::tools::ToolsConfig::new(["file_edit"])?;
        let tools = guffbolt::Tools::builder()
            .with_env(&self.env)
            .build(&config)?;
        let at = guffbolt::CallRef {
            step: 1,
            index: 0,
            call_id: "call-1".to_owned(),
        };
        let context = guffbolt::tools::Context {
            call_ref: &at,
            run_id: RUN_ID,
            wake: None,
        };
        Ok(tools.dispatch(&edit.name, &edit.arguments, context).await)
    }
}

/// A `file_edit` call replacing `old` with the twenty-four new lines.
fn file_edit(old: &str) -> Call {
    Call {
        name: "file_edit".to_owned(),
        arguments: serde_json::json!({"path": "src/a.rs", "old_text": old, "new_text": added()}),
        ..planned()
    }
}

/// The edit item's counts come from what a real edit did to a real file:
/// eight lines out and twenty-four in, read from the report the tool wrote,
/// never typed into the test.
#[tokio::test]
async fn a_real_file_edit_carries_the_lines_it_changed() -> Result<(), Box<dyn std::error::Error>> {
    let desk = Desk::new()?;
    let edit = file_edit(&removed());
    let output = desk.edit(&edit).await?;
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    ran(&ledger, &edit, Some(output));
    let (trace, records) = ledger.finish();
    frames.taken();

    let expected = vec![FileChange {
        path: "src/a.rs".into(),
        added: 24,
        removed: 8,
    }];
    assert_eq!(records.len(), 1, "one edit item");
    assert_eq!(
        (records[0].kind, records[0].status),
        (ItemKind::Edit, ItemStatus::Succeeded)
    );
    assert_eq!(records[0].changes, expected);
    assert_eq!(trace.calls[0].changes, expected);
    assert_eq!(
        std::fs::read_to_string(desk.directory.path().join("src/a.rs"))?,
        format!("fn keep() {{}}\n{}fn tail() {{}}\n", added()),
        "the file on disk holds the edit"
    );
    Ok(())
}

/// A real edit the tool refuses changes nothing, and its item counts nothing.
#[tokio::test]
async fn a_real_file_edit_that_finds_no_text_carries_no_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let desk = Desk::new()?;
    let edit = file_edit("text the file does not hold\n");
    let output = desk.edit(&edit).await?;
    assert!(output.error_code.is_some(), "{}", output.text);
    let frames = Frames::default();
    let sink = frames.sink();
    let scrub = scrub();
    let ledger = Ledger::new(RUN_ID, TURN, &sink, &scrub);

    ran(&ledger, &edit, Some(output));
    let (_trace, records) = ledger.finish();
    frames.taken();

    assert_eq!(
        (records[0].kind, records[0].status),
        (ItemKind::Edit, ItemStatus::Failed)
    );
    assert_eq!(records[0].changes, Vec::new());
    assert_eq!(
        std::fs::read_to_string(desk.directory.path().join("src/a.rs"))?,
        format!("fn keep() {{}}\n{}fn tail() {{}}\n", removed()),
        "the file is unchanged"
    );
    Ok(())
}
