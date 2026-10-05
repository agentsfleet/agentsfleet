#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::os::unix::fs::symlink;

use afr_executor::MAX_READ_BYTES;
use serde_json::{Value, json};

use crate::catalog::{Entry, FILE_APPEND, FILE_DELETE, FILE_EDIT, FILE_READ, FILE_WRITE};
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::sandbox::ScriptedExecutor;
use crate::testing::{Live, call, call_in, hosted, offered};

/// The argument names the calls spell.
const PATH: &str = "path";
const CONTENT: &str = "content";
const OLD_TEXT: &str = "old_text";
const NEW_TEXT: &str = "new_text";

/// The file the round trip works on, and what lands in it.
const TODO: &str = "notes/todo.txt";
const ONE: &str = "one\n";
const TWO: &str = "two\n";
/// What the five tools answer when their call lands.
const PATH_LEAVES: &str = "leaves the workspace";

/// The five plain file tools' names.
const FILE_TOOLS: [&Entry; 5] = [
    &FILE_READ,
    &FILE_WRITE,
    &FILE_APPEND,
    &FILE_DELETE,
    &FILE_EDIT,
];

/// Arguments for `entry` on `path`, with whatever else it takes.
fn on(entry: &Entry, path: &str) -> Value {
    match entry.name() {
        name if name == FILE_WRITE.name() || name == FILE_APPEND.name() => {
            json!({PATH: path, CONTENT: "x"})
        }
        name if name == FILE_EDIT.name() => {
            json!({PATH: path, OLD_TEXT: "x", NEW_TEXT: "y"})
        }
        _read_or_delete => json!({PATH: path}),
    }
}

#[tokio::test]
async fn test_file_tools_operate_under_workspace() {
    let live = Live::start().await;
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let mut lease = Lease::default();
    let mut answers = Vec::with_capacity(6);
    for (entry, arguments) in [
        (&FILE_WRITE, json!({PATH: TODO, CONTENT: ONE})),
        (&FILE_APPEND, json!({PATH: TODO, CONTENT: TWO})),
        (&FILE_READ, json!({PATH: TODO})),
        (
            &FILE_EDIT,
            json!({PATH: TODO, OLD_TEXT: "two", NEW_TEXT: "three"}),
        ),
        (&FILE_DELETE, json!({PATH: TODO})),
    ] {
        let tool = offered(&selection, entry);
        answers.push(call_in(tool, &live.client, &mut lease, arguments).await);
        if entry.name() == FILE_EDIT.name() {
            let on_disk = std::fs::read_to_string(live.root.join(TODO)).unwrap();
            assert_eq!(on_disk, "one\nthree\n", "the edit landed on disk");
        }
    }
    let absolute = format!("/workspace/{TODO}");
    let gone = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &mut lease,
        json!({PATH: absolute}),
    )
    .await;

    let texts: Vec<&str> = answers.iter().map(|answer| answer.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Wrote 4 bytes to notes/todo.txt",
            "Appended 4 bytes to notes/todo.txt",
            "one\ntwo\n",
            "Replaced 3 bytes with 5 bytes in notes/todo.txt",
            "Deleted notes/todo.txt",
        ]
    );
    for answer in &answers {
        assert_eq!(answer.error_code, None, "{answer:?}");
        assert_eq!(answer.exit_code, None, "{answer:?}");
    }
    assert_eq!(
        gone.error_code,
        Some(ToolErrorCode::FileNotFound),
        "{gone:?}"
    );
    assert!(!live.root.join(TODO).exists());
    live.stop().await;
}

/// A path that climbs out by name is refused before any call: the scripted
/// executor refuses every file call, and no refusal of its is read back.
#[tokio::test]
async fn test_file_tools_refuse_path_escape() {
    let executor = ScriptedExecutor::default();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let mut lease = Lease::default();

    for entry in FILE_TOOLS {
        for path in [
            "../etc/passwd",
            "/etc/passwd",
            "a/../../b",
            "/workspace/../x",
        ] {
            let refused = call_in(
                offered(&selection, entry),
                &executor,
                &mut lease,
                on(entry, path),
            )
            .await;

            assert_eq!(
                refused.error_code,
                Some(ToolErrorCode::PathNotAllowed),
                "{refused:?}"
            );
            assert!(refused.text.contains(PATH_LEAVES), "{}", refused.text);
        }
    }
    assert!(
        executor.spawned().is_empty(),
        "nothing was asked of the sandbox"
    );
}

/// A link that leaves the workspace is caught by the executor's handle, and
/// reads back under the same code; what it points at is untouched.
#[tokio::test]
async fn a_link_out_of_the_workspace_is_refused_by_the_executor_under_the_same_code() {
    let live = Live::start().await;
    let outside = live.outside();
    std::fs::write(outside.join("secret"), b"secret").unwrap();
    symlink(&outside, live.root.join("link")).unwrap();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let mut lease = Lease::default();

    let mut refusals = Vec::with_capacity(FILE_TOOLS.len());
    for entry in FILE_TOOLS {
        let tool = offered(&selection, entry);
        refusals.push(call_in(tool, &live.client, &mut lease, on(entry, "link/secret")).await);
    }

    for refused in &refusals {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::PathNotAllowed),
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
        .select(&[FILE_READ.name(), FILE_EDIT.name(), FILE_DELETE.name()])
        .unwrap();
    let mut lease = Lease::default();

    let mut refusals = Vec::with_capacity(3);
    for entry in [&FILE_READ, &FILE_EDIT, &FILE_DELETE] {
        let tool = offered(&selection, entry);
        refusals.push(call_in(tool, &live.client, &mut lease, on(entry, "absent.txt")).await);
    }

    for refused in &refusals {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::FileNotFound),
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
    let mut lease = Lease::default();

    let empty = call_in(
        tool,
        &live.client,
        &mut lease,
        json!({PATH: "a.txt", OLD_TEXT: "", NEW_TEXT: "x"}),
    )
    .await;
    let absent = call_in(
        tool,
        &live.client,
        &mut lease,
        json!({PATH: "a.txt", OLD_TEXT: "omega", NEW_TEXT: "x"}),
    )
    .await;

    assert_eq!(
        empty.error_code,
        Some(ToolErrorCode::InvalidArguments),
        "{empty:?}"
    );
    assert_eq!(
        absent.error_code,
        Some(ToolErrorCode::TextNotFound),
        "{absent:?}"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join("a.txt")).unwrap(),
        "alpha\n"
    );
    live.stop().await;
}

/// An edit replaces the first occurrence, as nullclaw's `file_edit` did.
#[tokio::test]
async fn an_edit_replaces_the_first_occurrence_only() {
    let live = Live::start().await;
    std::fs::write(live.root.join("a.txt"), "x y x\n").unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_EDIT.name()]).unwrap();
    let mut lease = Lease::default();

    let edited = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &mut lease,
        json!({PATH: "a.txt", OLD_TEXT: "x", NEW_TEXT: "z"}),
    )
    .await;

    assert_eq!(edited.text, "Replaced 1 bytes with 1 bytes in a.txt");
    assert_eq!(
        std::fs::read_to_string(live.root.join("a.txt")).unwrap(),
        "z y x\n"
    );
    live.stop().await;
}

/// A file longer than one read carries is read cut and says so, and is
/// refused for an edit, which would write it back cut.
#[tokio::test]
async fn a_file_past_one_read_is_cut_for_reading_and_refused_for_editing() {
    let live = Live::start().await;
    let length = usize::try_from(MAX_READ_BYTES).unwrap() + 1;
    std::fs::write(live.root.join("big.txt"), vec![b'x'; length]).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ.name(), FILE_EDIT.name()])
        .unwrap();
    let mut lease = Lease::default();

    let read = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &mut lease,
        json!({PATH: "big.txt"}),
    )
    .await;
    let edit = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &mut lease,
        json!({PATH: "big.txt", OLD_TEXT: "x", NEW_TEXT: "y"}),
    )
    .await;

    assert!(
        read.text.ends_with(&format!(
            "... the file continues past {MAX_READ_BYTES} bytes ..."
        )),
        "{}",
        read.text.len()
    );
    assert_eq!(read.error_code, None);
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{edit:?}"
    );
    live.stop().await;
}

#[tokio::test]
async fn a_file_that_is_not_text_is_read_lossily_and_refused_for_editing() {
    let live = Live::start().await;
    std::fs::write(live.root.join("blob"), [0xff, 0xfe, b'a']).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ.name(), FILE_EDIT.name()])
        .unwrap();
    let mut lease = Lease::default();

    let read = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &mut lease,
        json!({PATH: "blob"}),
    )
    .await;
    let edit = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &mut lease,
        json!({PATH: "blob", OLD_TEXT: "a", NEW_TEXT: "b"}),
    )
    .await;

    assert_eq!(read.text, "\u{fffd}\u{fffd}a");
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::InvalidArguments),
        "{edit:?}"
    );
    assert!(edit.text.contains("is not text"), "{}", edit.text);
    live.stop().await;
}

#[tokio::test]
async fn an_empty_path_is_invalid_and_a_call_without_a_sandbox_is_refused() {
    let executor = ScriptedExecutor::default();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let mut lease = Lease::default();

    for entry in FILE_TOOLS {
        let tool = offered(&selection, entry);
        let empty: ToolOutput = call_in(tool, &executor, &mut lease, on(entry, "")).await;
        let unsandboxed = call(tool, &mut lease, on(entry, "a.txt")).await;

        assert_eq!(
            empty.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{empty:?}"
        );
        assert_eq!(
            unsandboxed.error_code,
            Some(ToolErrorCode::SandboxUnavailable),
            "{unsandboxed:?}"
        );
    }
}

/// A refusal that is neither a path out nor a missing name reads back as the
/// sandbox being unavailable, in the executor's own words.
#[tokio::test]
async fn a_directory_given_as_a_file_reads_the_executors_own_sentence() {
    let live = Live::start().await;
    std::fs::create_dir(live.root.join("dir")).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_READ.name()]).unwrap();
    let mut lease = Lease::default();

    let refused = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &mut lease,
        json!({PATH: "dir"}),
    )
    .await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SandboxUnavailable),
        "{refused:?}"
    );
    assert!(
        refused.text.contains("not a regular file"),
        "{}",
        refused.text
    );
    live.stop().await;
}

/// Over every path built from these parts, what the gate lets through is
/// relative and never climbs, and what it refuses carries the code.
#[test]
fn every_accepted_path_is_relative_and_never_climbs() {
    const PARTS: [&str; 7] = ["", ".", "..", "a", "a b", "/workspace", "/etc"];
    let mut accepted = 0;
    let mut refused = 0;
    for first in PARTS {
        for second in PARTS {
            for third in PARTS {
                let path = [first, second, third].join("/");
                match super::inside(&path) {
                    Ok(relative) => {
                        accepted += 1;
                        let relative = std::path::Path::new(relative);
                        assert!(!relative.is_absolute(), "{path:?} -> {relative:?}");
                        assert!(
                            !relative
                                .components()
                                .any(|part| part == std::path::Component::ParentDir),
                            "{path:?} -> {relative:?}"
                        );
                    }
                    Err(output) => {
                        refused += 1;
                        assert!(
                            matches!(
                                output.error_code,
                                Some(
                                    ToolErrorCode::PathNotAllowed | ToolErrorCode::InvalidArguments
                                )
                            ),
                            "{path:?} -> {output:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        accepted > 0 && refused > 0,
        "{accepted} accepted, {refused} refused"
    );
    assert_eq!(super::inside("/workspace/a/b").unwrap(), "a/b");
    assert_eq!(super::inside("a/./b").unwrap(), "a/./b");
}
