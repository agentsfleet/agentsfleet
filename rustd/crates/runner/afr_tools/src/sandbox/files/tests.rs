#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::os::unix::fs::symlink;

use serde_json::{Value, json};

use crate::catalog::{Entry, FILE_EDIT, FILE_READ, FILE_WRITE};
use crate::run::Run;
use crate::runtime::ToolErrorCode;
use crate::sandbox::ScriptedGuest;
use crate::testing::{Live, call_in, hosted, offered};

/// The argument names the calls spell.
pub(super) const PATH: &str = "path";
const CONTENT: &str = "content";
const EDITS: &str = "edits";
const OLD_TEXT: &str = "old_text";
const NEW_TEXT: &str = "new_text";

/// The file the round trip works on, and what lands in it.
const TODO: &str = "notes/todo.txt";
const ONE: &str = "one\n";
const TWO: &str = "two\n";
/// What the three tools answer when their path leaves the workspace.
const PATH_LEAVES: &str = "leaves the working directory";

/// The three plain file tools' names.
pub(super) const FILE_TOOLS: [&Entry; 3] = [&FILE_READ, &FILE_WRITE, &FILE_EDIT];

/// `file_edit`'s arguments on `path`: one edit replacing `old` with `new`.
pub(super) fn edit(path: &str, old: &str, new: &str) -> Value {
    json!({PATH: path, EDITS: [{OLD_TEXT: old, NEW_TEXT: new}]})
}

/// Arguments for `entry` on `path`, with whatever else it takes.
pub(super) fn on(entry: &Entry, path: &str) -> Value {
    match entry.name() {
        name if name == FILE_WRITE.name() => json!({PATH: path, CONTENT: "x"}),
        name if name == FILE_EDIT.name() => edit(path, "x", "y"),
        _read => json!({PATH: path}),
    }
}

#[tokio::test]
async fn test_file_tools_operate_under_workspace() {
    let live = Live::start().await;
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let run = Run::default();
    let mut answers = Vec::with_capacity(4);
    for (entry, arguments) in [
        (
            &FILE_WRITE,
            json!({PATH: TODO, CONTENT: format!("{ONE}{TWO}")}),
        ),
        (&FILE_READ, json!({PATH: TODO})),
        (&FILE_EDIT, edit(TODO, "two", "three")),
        (&FILE_WRITE, json!({PATH: TODO, CONTENT: ONE})),
    ] {
        let tool = offered(&selection, entry);
        answers.push(call_in(tool, &live.client, &run, arguments).await);
        if entry.name() == FILE_EDIT.name() {
            let on_disk = std::fs::read_to_string(live.root.join(TODO)).unwrap();
            assert_eq!(on_disk, "one\nthree\n", "the edit landed on disk");
        }
    }
    let absolute = format!("/workspace/{TODO}");
    let reread = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &run,
        json!({PATH: absolute}),
    )
    .await;

    let texts: Vec<&str> = answers.iter().map(|answer| answer.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Wrote 8 bytes to notes/todo.txt\nLines changed:\n\"notes/todo.txt\" +2 -0",
            "one\ntwo\n",
            "Applied 1 edit to notes/todo.txt\nLines changed:\n\"notes/todo.txt\" +1 -1",
            "Wrote 4 bytes to notes/todo.txt\nLines changed:\n\"notes/todo.txt\" +0 -1",
        ]
    );
    for answer in &answers {
        assert_eq!(answer.error_code, None, "{answer:?}");
        assert_eq!(answer.exit_code, None, "{answer:?}");
    }
    assert_eq!(
        reread.text, ONE,
        "an absolute path under /workspace names the file"
    );
    assert_eq!(reread.error_code, None, "{reread:?}");
    live.stop().await;
}

/// A path that climbs out by name is refused before any call: the scripted
/// guest agent refuses every file call, and no refusal of its is read back.
#[tokio::test]
async fn test_file_tools_refuse_path_escape() {
    let agent = ScriptedGuest::default();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let run = Run::default();

    for entry in FILE_TOOLS {
        for path in [
            "../etc/passwd",
            "/etc/passwd",
            "a/../../b",
            "/workspace/../x",
        ] {
            let refused = call_in(offered(&selection, entry), &agent, &run, on(entry, path)).await;

            assert_eq!(
                refused.error_code,
                Some(ToolErrorCode::PathNotAllowed.into()),
                "{refused:?}"
            );
            assert!(refused.text.contains(PATH_LEAVES), "{}", refused.text);
        }
    }
    assert!(
        agent.spawned().is_empty(),
        "nothing was asked of the sandbox"
    );
}

/// A sandbox that cannot serve a file call reads back as unavailable, never as
/// a missing file or a refused path: the caller's next step differs.
#[tokio::test]
async fn a_sandbox_that_cannot_serve_a_file_call_reads_back_as_unavailable() {
    let agent = ScriptedGuest::default();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let run = Run::default();

    for entry in FILE_TOOLS {
        let failed = call_in(offered(&selection, entry), &agent, &run, on(entry, TODO)).await;

        assert_eq!(
            failed.error_code,
            Some(ToolErrorCode::SandboxUnavailable.into()),
            "{}: {failed:?}",
            entry.name()
        );
        assert!(
            !failed.text.is_empty(),
            "the guest agent's sentence is carried"
        );
    }
}

/// A link that leaves the workspace is caught by the guest agent's handle, and
/// reads back under the same code; what it points at is untouched.
#[tokio::test]
async fn a_link_out_of_the_workspace_is_refused_by_the_agent_under_the_same_code() {
    let live = Live::start().await;
    let outside = live.outside();
    std::fs::write(outside.join("secret"), b"secret").unwrap();
    symlink(&outside, live.root.join("link")).unwrap();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let run = Run::default();

    let mut refusals = Vec::with_capacity(FILE_TOOLS.len());
    for entry in FILE_TOOLS {
        let tool = offered(&selection, entry);
        refusals.push(call_in(tool, &live.client, &run, on(entry, "link/secret")).await);
    }

    for refused in &refusals {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::PathNotAllowed.into()),
            "{refused:?}"
        );
        assert!(
            refused.text.contains("outside the workspace"),
            "{}",
            refused.text
        );
    }
    assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"secret");
    live.stop().await;
}

#[tokio::test]
async fn a_name_the_workspace_does_not_have_reads_file_not_found() {
    let live = Live::start().await;
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ.name(), FILE_EDIT.name()])
        .unwrap();
    let run = Run::default();

    let mut refusals = Vec::with_capacity(2);
    for entry in [&FILE_READ, &FILE_EDIT] {
        let tool = offered(&selection, entry);
        refusals.push(call_in(tool, &live.client, &run, on(entry, "absent.txt")).await);
    }

    for refused in &refusals {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::FileNotFound.into()),
            "{refused:?}"
        );
        assert!(refused.text.contains("no such file"), "{}", refused.text);
    }
    live.stop().await;
}

#[tokio::test]
async fn an_edit_needs_old_text_and_finds_it_or_changes_nothing() {
    let live = Live::start().await;
    std::fs::write(live.root.join("a.txt"), "alpha\n").unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_EDIT.name()]).unwrap();
    let tool = offered(&selection, &FILE_EDIT);
    let run = Run::default();

    let empty = call_in(tool, &live.client, &run, edit("a.txt", "", "x")).await;
    let absent = call_in(tool, &live.client, &run, edit("a.txt", "omega", "x")).await;

    assert_eq!(
        empty.error_code,
        Some(ToolErrorCode::InvalidArguments.into()),
        "{empty:?}"
    );
    assert_eq!(
        absent.error_code,
        Some(ToolErrorCode::TextNotFound.into()),
        "{absent:?}"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join("a.txt")).unwrap(),
        "alpha\n"
    );
    live.stop().await;
}

/// Text the file holds twice is refused, and neither place is changed.
#[tokio::test]
async fn an_edit_refuses_text_the_file_holds_twice() {
    let live = Live::start().await;
    std::fs::write(live.root.join("a.txt"), "x y x\n").unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_EDIT.name()]).unwrap();
    let run = Run::default();

    let refused = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &run,
        edit("a.txt", "x", "z"),
    )
    .await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::TextNotUnique.into()),
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join("a.txt")).unwrap(),
        "x y x\n"
    );
    live.stop().await;
}

/// A refusal that is neither a path out nor a missing name reads back as the
/// sandbox being unavailable, in the guest agent's own words.
#[tokio::test]
async fn a_directory_given_as_a_file_reads_the_agents_own_sentence() {
    let live = Live::start().await;
    std::fs::create_dir(live.root.join("dir")).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_READ.name()]).unwrap();
    let run = Run::default();

    let refused = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &run,
        json!({PATH: "dir"}),
    )
    .await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SandboxUnavailable.into()),
        "{refused:?}"
    );
    assert!(
        refused.text.contains("not a regular file"),
        "{}",
        refused.text
    );
    live.stop().await;
}
