//! The file tools over a real directory: `LocalEnv` rooted in a scratch
//! directory, the tools built with `Tools::builder().with_env`, and each call
//! dispatched the way a loop dispatches it. Every answer is read as the model
//! reads it, and every write is checked on disk.

use std::sync::Arc;

use guffbolt::tools::search::{Deferred, ToolSearch};
use guffbolt::tools::{Context, ContextTool, LocalEnv, ToolsConfig, Typed};
use guffbolt::{CallRef, ImageKind, ToolOutput, ToolSpec, Tools};
use serde_json::{Value, json};
use tempfile::TempDir;

/// What one read of the environment carries: 8 MiB.
const ONE_READ: usize = 8 * 1024 * 1024;
/// The most an image may weigh: 5 MiB.
const IMAGE_MAX: usize = 5 * 1024 * 1024;
/// What `ls` and `find` say when they stop at their limit.
const LIMIT_REACHED: &str = "... limit reached; narrow the path or the pattern";
/// What a refused path reads back after it.
const LEAVES: &str = "leaves the working directory";

/// A scratch working directory and the environment rooted in it.
struct Desk {
    directory: TempDir,
    env: LocalEnv,
}

impl Desk {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let env = LocalEnv::new(directory.path()).expect("the directory exists");
        Self { directory, env }
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.directory.path().join(relative)
    }

    /// Writes `bytes` at `relative`, making its directories.
    fn put(&self, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("directories made");
        }
        std::fs::write(path, bytes).expect("written");
    }

    /// The text at `relative`, or `None` when nothing is there.
    fn text(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.path(relative)).ok()
    }

    /// Every file tool, `image` showing pictures when `image_input` says so.
    fn tools(&self, image_input: bool) -> Tools<ContextTool<'_>> {
        let config = ToolsConfig::new(["file_*", "apply_patch", "image", "ls", "find", "grep"])
            .expect("names and patterns");
        Tools::builder()
            .with_env(&self.env)
            .with_image_input(image_input)
            .build(&config)
            .expect("the environment is given")
    }
}

/// Calls `name` with `arguments` the way a loop dispatches a call.
async fn call(tools: &Tools<ContextTool<'_>>, name: &str, arguments: Value) -> ToolOutput {
    let call_ref = CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };
    tools.dispatch(name, &arguments, context).await
}

/// The code a failed call carries, by its spelling.
fn code(output: &ToolOutput) -> Option<&'static str> {
    output.error_code.map(guffbolt::ErrorCode::as_str)
}

// ---- the path gate ------------------------------------------------------

/// Every file tool passes its path through one gate: a path that names no
/// file, climbs out through `..` or is absolute outside the root is refused
/// before any read or write.
#[tokio::test]
async fn a_path_that_leaves_the_working_directory_is_refused_before_any_write() {
    let desk = Desk::new();
    let tools = desk.tools(false);

    let empty = call(&tools, "file_write", json!({"path": "", "content": "x"})).await;
    let here = call(&tools, "file_write", json!({"path": ".", "content": "x"})).await;
    let climbs = call(
        &tools,
        "file_write",
        json!({"path": "a/../../x", "content": "x"}),
    )
    .await;
    let absolute = call(
        &tools,
        "file_append",
        json!({"path": "/etc/hosts", "content": "x"}),
    )
    .await;
    let read = call(&tools, "file_read", json!({"path": "../x"})).await;

    let no_file = "[invalid_arguments] path must name a file in the working directory";
    assert_eq!(empty.text, no_file);
    assert_eq!(here.text, no_file);
    assert_eq!(
        climbs.text,
        format!("[path_not_allowed] a/../../x {LEAVES}")
    );
    assert_eq!(
        absolute.text,
        format!("[path_not_allowed] /etc/hosts {LEAVES}")
    );
    assert_eq!(read.text, format!("[path_not_allowed] ../x {LEAVES}"));
    let written: Vec<_> = std::fs::read_dir(desk.directory.path())
        .expect("listed")
        .collect();
    assert_eq!(written.len(), 0, "nothing was written");
}

/// An absolute path under the root names the file below it.
#[tokio::test]
async fn an_absolute_path_under_the_root_names_the_file_below_it() {
    let desk = Desk::new();
    let tools = desk.tools(false);
    let root = desk
        .directory
        .path()
        .canonicalize()
        .expect("the root resolves");
    let absolute = format!("{}/notes/a.txt", root.display());

    let output = call(
        &tools,
        "file_write",
        json!({"path": absolute, "content": "a\n"}),
    )
    .await;

    assert_eq!(
        output.text,
        "Wrote 2 bytes to notes/a.txt\nLines changed:\n\"notes/a.txt\" +1 -0"
    );
    assert_eq!(desk.text("notes/a.txt").as_deref(), Some("a\n"));
}

/// A link that leads out of the root is the environment's to refuse; its
/// refusal reads back under the gate's code.
#[cfg(unix)]
#[tokio::test]
async fn a_link_out_of_the_root_is_refused_under_the_gates_code() {
    let desk = Desk::new();
    let outside = tempfile::tempdir().expect("another scratch directory");
    std::fs::write(outside.path().join("secret.txt"), "s\n").expect("written");
    std::os::unix::fs::symlink(outside.path(), desk.path("out")).expect("linked");
    let tools = desk.tools(false);

    let read = call(&tools, "file_read", json!({"path": "out/secret.txt"})).await;
    let write = call(
        &tools,
        "file_write",
        json!({"path": "out/new.txt", "content": "x"}),
    )
    .await;

    assert_eq!(
        read.text,
        "[path_not_allowed] the path leaves the working directory"
    );
    assert_eq!(code(&write), Some("path_not_allowed"));
    assert!(
        !outside.path().join("new.txt").exists(),
        "nothing was written outside"
    );
}

// ---- file_write, file_append, file_delete, file_edit --------------------

#[tokio::test]
async fn file_write_makes_directories_and_counts_the_lines_it_replaced() {
    let desk = Desk::new();
    let tools = desk.tools(false);

    let first = call(
        &tools,
        "file_write",
        json!({"path": "deep/dir/n.txt", "content": "a\nb\n"}),
    )
    .await;
    let second = call(
        &tools,
        "file_write",
        json!({"path": "deep/dir/n.txt", "content": "a\nc\n"}),
    )
    .await;

    assert_eq!(
        first.text,
        "Wrote 4 bytes to deep/dir/n.txt\nLines changed:\n\"deep/dir/n.txt\" +2 -0"
    );
    assert_eq!(
        second.text,
        "Wrote 4 bytes to deep/dir/n.txt\nLines changed:\n\"deep/dir/n.txt\" +1 -1"
    );
    assert_eq!(desk.text("deep/dir/n.txt").as_deref(), Some("a\nc\n"));
}

/// A file that cannot be read whole as text is replaced uncounted: no
/// report follows the answer.
#[tokio::test]
async fn file_write_over_a_file_it_cannot_count_reports_no_lines() {
    let desk = Desk::new();
    desk.put("bin.dat", [0xff_u8, 0xfe]);
    desk.put("big.txt", "x".repeat(ONE_READ + 1));
    let tools = desk.tools(false);

    let binary = call(
        &tools,
        "file_write",
        json!({"path": "bin.dat", "content": "ok"}),
    )
    .await;
    let big = call(
        &tools,
        "file_write",
        json!({"path": "big.txt", "content": "small\n"}),
    )
    .await;

    assert_eq!(binary.text, "Wrote 2 bytes to bin.dat");
    assert_eq!(big.text, "Wrote 6 bytes to big.txt");
    assert_eq!(desk.text("bin.dat").as_deref(), Some("ok"));
    assert_eq!(desk.text("big.txt").as_deref(), Some("small\n"));
}

/// A write to a path that is a directory fails in the system's words, as
/// the environment being unable to do it.
#[tokio::test]
async fn file_write_over_a_directory_fails_and_leaves_it() {
    let desk = Desk::new();
    std::fs::create_dir(desk.path("dir")).expect("made");
    let tools = desk.tools(false);

    let output = call(&tools, "file_write", json!({"path": "dir", "content": "x"})).await;

    assert_eq!(code(&output), Some("sandbox_unavailable"));
    assert!(desk.path("dir").is_dir(), "the directory is still there");
}

#[tokio::test]
async fn file_append_makes_the_file_then_adds_to_its_end() {
    let desk = Desk::new();
    let tools = desk.tools(false);

    let first = call(
        &tools,
        "file_append",
        json!({"path": "logs/run.log", "content": "one\n"}),
    )
    .await;
    let second = call(
        &tools,
        "file_append",
        json!({"path": "logs/run.log", "content": "zwei \u{1F600}\n"}),
    )
    .await;
    let refused = call(
        &tools,
        "file_append",
        json!({"path": "../run.log", "content": "x"}),
    )
    .await;

    assert_eq!(
        first.text,
        "Appended 4 bytes to logs/run.log\nLines changed:\n\"logs/run.log\" +1 -0"
    );
    assert_eq!(
        second.text,
        "Appended 10 bytes to logs/run.log\nLines changed:\n\"logs/run.log\" +1 -0"
    );
    assert_eq!(
        refused.text,
        format!("[path_not_allowed] ../run.log {LEAVES}")
    );
    assert_eq!(
        desk.text("logs/run.log").as_deref(),
        Some("one\nzwei \u{1F600}\n")
    );
}

#[tokio::test]
async fn file_append_onto_a_directory_fails_in_the_systems_words() {
    let desk = Desk::new();
    std::fs::create_dir(desk.path("dir")).expect("made");
    let tools = desk.tools(false);

    let output = call(
        &tools,
        "file_append",
        json!({"path": "dir", "content": "x"}),
    )
    .await;

    assert_eq!(code(&output), Some("sandbox_unavailable"));
}

#[tokio::test]
async fn file_delete_removes_a_file_and_refuses_what_is_not_one() {
    let desk = Desk::new();
    desk.put("gone.txt", "x");
    std::fs::create_dir(desk.path("dir")).expect("made");
    let tools = desk.tools(false);

    let deleted = call(&tools, "file_delete", json!({"path": "gone.txt"})).await;
    let again = call(&tools, "file_delete", json!({"path": "gone.txt"})).await;
    let directory = call(&tools, "file_delete", json!({"path": "dir"})).await;
    let outside = call(&tools, "file_delete", json!({"path": "../gone.txt"})).await;

    assert_eq!(deleted.text, "Deleted gone.txt");
    assert!(!desk.path("gone.txt").exists());
    assert_eq!(code(&again), Some("file_not_found"));
    assert_eq!(
        directory.text,
        "[sandbox_unavailable] dir is not a regular file"
    );
    assert!(desk.path("dir").is_dir(), "the directory stays");
    assert_eq!(
        outside.text,
        format!("[path_not_allowed] ../gone.txt {LEAVES}")
    );
}

#[tokio::test]
async fn file_edit_replaces_the_first_occurrence_and_counts_the_change() {
    let desk = Desk::new();
    desk.put("e.txt", "keep\nx and x\nkeep\n");
    let tools = desk.tools(false);

    let output = call(
        &tools,
        "file_edit",
        json!({"path": "e.txt", "old_text": "x", "new_text": "\u{00e9}t\u{00e9}"}),
    )
    .await;

    assert_eq!(
        output.text,
        "Replaced 1 bytes with 5 bytes in e.txt\nLines changed:\n\"e.txt\" +1 -1"
    );
    assert_eq!(
        desk.text("e.txt").as_deref(),
        Some("keep\n\u{00e9}t\u{00e9} and x\nkeep\n")
    );
}

/// Each way an edit is refused leaves the file as it was.
#[tokio::test]
async fn file_edit_refuses_what_it_cannot_edit_and_leaves_the_file() {
    let desk = Desk::new();
    desk.put("e.txt", "alpha\n");
    desk.put("bin.dat", [0xff_u8, 0x00, 0xfe]);
    desk.put("big.txt", "y".repeat(ONE_READ + 1));
    let tools = desk.tools(false);
    let edit = |path: &str, old: &str| json!({"path": path, "old_text": old, "new_text": "z"});

    let empty = call(&tools, "file_edit", edit("e.txt", "")).await;
    let absent = call(&tools, "file_edit", edit("e.txt", "omega")).await;
    let binary = call(&tools, "file_edit", edit("bin.dat", "a")).await;
    let big = call(&tools, "file_edit", edit("big.txt", "y")).await;
    let missing = call(&tools, "file_edit", edit("nowhere.txt", "a")).await;
    let unknown = call(
        &tools,
        "file_edit",
        json!({"path": "e.txt", "old_text": "a", "new_txt": "z"}),
    )
    .await;

    assert_eq!(empty.text, "[invalid_arguments] old_text must not be empty");
    assert_eq!(
        absent.text,
        "[text_not_found] e.txt does not contain old_text"
    );
    assert_eq!(
        binary.text,
        "[invalid_arguments] bin.dat is not text, so it cannot be edited"
    );
    assert_eq!(
        big.text,
        "[file_too_large] big.txt is longer than 8388608 bytes"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert!(
        unknown
            .text
            .starts_with("[invalid_arguments] unknown field `new_txt`"),
        "{}",
        unknown.text
    );
    assert_eq!(desk.text("e.txt").as_deref(), Some("alpha\n"));
}

// ---- file_read ----------------------------------------------------------

#[tokio::test]
async fn file_read_pages_a_window_and_says_where_to_read_on() {
    let desk = Desk::new();
    desk.put("r.txt", "a\nb\nc\nd\ne\n");
    let tools = desk.tools(false);

    let window = call(
        &tools,
        "file_read",
        json!({"path": "r.txt", "offset": 2, "limit": 2}),
    )
    .await;
    let rest = call(&tools, "file_read", json!({"path": "r.txt", "offset": 4})).await;
    let whole = call(&tools, "file_read", json!({"path": "r.txt"})).await;

    assert_eq!(
        window.text,
        "b\nc\n... the file continues at line 4; read on with offset 4"
    );
    assert_eq!(rest.text, "d\ne\n");
    assert_eq!(whole.text, "a\nb\nc\nd\ne\n");
}

#[tokio::test]
async fn file_read_refuses_an_offset_past_the_end_or_zero() {
    let desk = Desk::new();
    desk.put("r.txt", "a\nb\n");
    desk.put("empty.txt", "");
    let tools = desk.tools(false);

    let past = call(&tools, "file_read", json!({"path": "r.txt", "offset": 9})).await;
    let past_empty = call(
        &tools,
        "file_read",
        json!({"path": "empty.txt", "offset": 2}),
    )
    .await;
    let first_of_empty = call(&tools, "file_read", json!({"path": "empty.txt"})).await;
    let zero = call(&tools, "file_read", json!({"path": "r.txt", "offset": 0})).await;
    let missing = call(&tools, "file_read", json!({"path": "nowhere.txt"})).await;

    assert_eq!(
        past.text,
        "[invalid_arguments] offset 9 is past the end of r.txt, which has 2 lines"
    );
    assert_eq!(
        past_empty.text,
        "[invalid_arguments] offset 2 is past the end of empty.txt, which has 0 lines"
    );
    assert_eq!(first_of_empty.text, "");
    assert_eq!(code(&first_of_empty), None);
    assert!(
        zero.text
            .starts_with("[invalid_arguments] invalid value: integer `0`"),
        "{}",
        zero.text
    );
    assert_eq!(code(&missing), Some("file_not_found"));
}

/// A line longer than the whole output budget is cut on a character
/// boundary, said so, and the read still moves on.
#[tokio::test]
async fn file_read_cuts_a_line_longer_than_the_budget_on_a_character_boundary() {
    let desk = Desk::new();
    // 13,334 three-byte characters: 40,002 bytes, past the 40,000-byte budget.
    let long = "\u{20ac}".repeat(13_334);
    desk.put("wide.txt", format!("{long}\ntail\n"));
    let tools = desk.tools(false);

    let output = call(&tools, "file_read", json!({"path": "wide.txt"})).await;

    let kept = "\u{20ac}".repeat(13_333);
    assert_eq!(
        output.text,
        format!(
            "{kept}\n... line 1 was cut at 39999 of its 40003 bytes\n... the file continues at \
             line 2; read on with offset 2"
        )
    );
}

/// A file longer than one read is paged up to there, and its last line
/// names `shell` for the rest.
#[tokio::test]
async fn file_read_past_one_read_names_shell_for_the_rest() {
    let desk = Desk::new();
    // 4,194,304 two-byte lines fill one read exactly; one more line is past it.
    desk.put("big.txt", format!("{}more\n", "x\n".repeat(ONE_READ / 2)));
    let tools = desk.tools(false);

    let last = call(
        &tools,
        "file_read",
        json!({"path": "big.txt", "offset": 4_194_304}),
    )
    .await;
    let past = call(
        &tools,
        "file_read",
        json!({"path": "big.txt", "offset": 4_194_305}),
    )
    .await;

    assert_eq!(
        last.text,
        "x\n... the file continues past 8388608 bytes; read the rest with shell"
    );
    assert_eq!(
        past.text,
        "[invalid_arguments] offset 4194305 is past the 4194304 lines of the first 8388608 \
         bytes of big.txt; read the rest with shell"
    );
}

// ---- file_read_hashed and file_edit_hashed ------------------------------

/// The tag a `file_read_hashed` answer gives line `number`, as `L<n>:<hhh>`.
fn tag_of(read: &str, number: usize) -> String {
    let prefix = format!("L{number}:");
    read.lines()
        .find(|line| line.starts_with(&prefix))
        .and_then(|line| line.split_once('|'))
        .map(|(tag, _text)| tag.to_owned())
        .expect("the read tags the line")
}

#[tokio::test]
async fn file_read_hashed_tags_every_line_by_its_text_and_the_line_before() {
    let desk = Desk::new();
    desk.put("h.txt", "alpha\nbeta\n");
    let tools = desk.tools(false);

    let read = call(&tools, "file_read_hashed", json!({"path": "h.txt"})).await;
    let missing = call(&tools, "file_read_hashed", json!({"path": "nowhere.txt"})).await;
    let outside = call(&tools, "file_read_hashed", json!({"path": "../h.txt"})).await;

    assert_eq!(read.text, "L1:94d|alpha\nL2:757|beta\nL3:261|\n");
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(code(&outside), Some("path_not_allowed"));
}

#[tokio::test]
async fn file_edit_hashed_replaces_a_tagged_line_and_a_tagged_range() {
    let desk = Desk::new();
    desk.put("h.txt", "one\ntwo\nthree\nfour\n");
    let tools = desk.tools(false);
    let read = call(&tools, "file_read_hashed", json!({"path": "h.txt"})).await;
    let (two, three) = (tag_of(&read.text, 2), tag_of(&read.text, 3));

    let line = call(
        &tools,
        "file_edit_hashed",
        json!({"path": "h.txt", "target": two, "new_text": "TWO"}),
    )
    .await;
    let after_line = desk.text("h.txt");
    let reread = call(&tools, "file_read_hashed", json!({"path": "h.txt"})).await;
    let range = call(
        &tools,
        "file_edit_hashed",
        json!({
            "path": "h.txt",
            "target": tag_of(&reread.text, 3),
            "end_target": tag_of(&reread.text, 4),
            "new_text": "",
        }),
    )
    .await;

    assert_eq!(
        line.text,
        format!(
            "Edited h.txt: {two} was line 2, 1 lines replaced\nLines changed:\n\"h.txt\" +1 -1"
        )
    );
    assert_eq!(
        after_line.as_deref(),
        Some("one\nTWO\nthree\nfour\n"),
        "a newline keeps the next line its own"
    );
    assert_eq!(
        range.text,
        format!(
            "Edited h.txt: {} was line 3, 2 lines replaced\nLines changed:\n\"h.txt\" +0 -2",
            tag_of(&reread.text, 3)
        )
    );
    assert_eq!(desk.text("h.txt").as_deref(), Some("one\nTWO\n"));
    assert_ne!(
        three,
        tag_of(&reread.text, 3),
        "the edit above changed line 3's parent, so its tag"
    );
}

/// A tag still found within reach after lines moved above it names its line
/// where it is now.
#[tokio::test]
async fn file_edit_hashed_finds_a_tag_after_lines_moved_above_it() {
    let desk = Desk::new();
    desk.put("h.txt", "a\nb\nc\nd\n");
    let tools = desk.tools(false);
    let read = call(&tools, "file_read_hashed", json!({"path": "h.txt"})).await;
    let three = tag_of(&read.text, 3);
    desk.put("h.txt", "inserted\na\nb\nc\nd\n");

    let output = call(
        &tools,
        "file_edit_hashed",
        json!({"path": "h.txt", "target": three, "new_text": "C"}),
    )
    .await;

    assert_eq!(
        output.text,
        format!(
            "Edited h.txt: {three} was line 4, 1 lines replaced\nLines changed:\n\"h.txt\" +1 -1"
        )
    );
    assert_eq!(
        desk.text("h.txt").as_deref(),
        Some("inserted\na\nb\nC\nd\n")
    );
}

/// A file that ends in a newline ends in an empty line, which a read tags
/// like any other, so the line edited before it is followed by a line and
/// keeps its newline.
#[tokio::test]
async fn file_edit_hashed_keeps_the_files_last_newline_when_it_edits_the_last_line() {
    let desk = Desk::new();
    desk.put("h.txt", "a\nb\nc\n");
    let tools = desk.tools(false);

    // L3:6a8 tags `c` after `b`.
    let output = call(
        &tools,
        "file_edit_hashed",
        json!({"path": "h.txt", "target": "L3:6a8", "new_text": "C"}),
    )
    .await;

    assert_eq!(code(&output), None, "{}", output.text);
    assert_eq!(desk.text("h.txt").as_deref(), Some("a\nb\nC\n"));
}

/// An end tag found before the start tag is refused, each argument named
/// with the tag it was given.
#[tokio::test]
async fn file_edit_hashed_names_each_argument_with_its_own_tag_when_the_end_comes_first() {
    let desk = Desk::new();
    desk.put("h.txt", "same\nsame\nsame\nother\n");
    let tools = desk.tools(false);

    // L4:df5 tags `other` after `same`; a85 is `same` after nothing, line 1.
    let output = call(
        &tools,
        "file_edit_hashed",
        json!({"path": "h.txt", "target": "L4:df5", "end_target": "L4:a85", "new_text": "x"}),
    )
    .await;

    assert_eq!(
        output.text,
        "[hash_mismatch] end_target L4:a85 was found before target L4:df5"
    );
    assert_eq!(
        desk.text("h.txt").as_deref(),
        Some("same\nsame\nsame\nother\n")
    );
}

/// Every tag the edit cannot place is refused, and the file stays.
#[tokio::test]
async fn file_edit_hashed_refuses_a_tag_it_cannot_place() {
    let desk = Desk::new();
    desk.put("h.txt", "same\nsame\nsame\nother\n");
    let tools = desk.tools(false);
    let read = call(&tools, "file_read_hashed", json!({"path": "h.txt"})).await;
    let (one, two, four) = (
        tag_of(&read.text, 1),
        tag_of(&read.text, 2),
        tag_of(&read.text, 4),
    );
    let hash_of = |tag: &str| {
        tag.split_once(':')
            .map(|(_, hash)| hash.to_owned())
            .expect("a hash")
    };
    let edit = |target: &str, end: Option<&str>| json!({"path": "h.txt", "target": target, "end_target": end, "new_text": "x"});

    let mut malformed = Vec::new();
    for tag in [
        "",
        "X1:abc",
        "L0:abc",
        "L1:abcd",
        "L1:",
        "Lone:abc",
        "L\u{00e9}:abc",
    ] {
        malformed.push(call(&tools, "file_edit_hashed", edit(tag, None)).await.text);
    }
    let past = call(&tools, "file_edit_hashed", edit("L9:abc", None)).await;
    let twice = call(&tools, "file_edit_hashed", edit(&two, None)).await;
    let stale_hash = if hash_of(&four) == "000" {
        "001"
    } else {
        "000"
    };
    let stale = call(
        &tools,
        "file_edit_hashed",
        edit(&format!("L4:{stale_hash}"), None),
    )
    .await;
    let backwards = call(&tools, "file_edit_hashed", edit(&four, Some(&one))).await;
    let bad_end = call(&tools, "file_edit_hashed", edit(&one, Some("L2"))).await;

    let not_a_tag = |tag: &str| {
        format!(
            "[invalid_arguments] {tag:?} is not a tag; one reads L<line>:<hash>, as \
             file_read_hashed printed it"
        )
    };
    assert_eq!(
        malformed,
        [
            "",
            "X1:abc",
            "L0:abc",
            "L1:abcd",
            "L1:",
            "Lone:abc",
            "L\u{00e9}:abc"
        ]
        .map(not_a_tag)
    );
    assert_eq!(
        past.text,
        "[hash_mismatch] target L9:abc is past the end of h.txt, which has 5 lines"
    );
    assert_eq!(
        twice.text,
        format!(
            "[hash_mismatch] target {two} matches h.txt more than once near line 2; read it again"
        )
    );
    assert_eq!(
        stale.text,
        format!(
            "[hash_mismatch] target L4:{stale_hash} no longer matches h.txt near line 4; the \
             file changed, read it again"
        )
    );
    assert_eq!(
        backwards.text,
        "[hash_mismatch] end_target L1 comes before target L4"
    );
    assert_eq!(bad_end.text, not_a_tag("L2"));
    assert_eq!(
        desk.text("h.txt").as_deref(),
        Some("same\nsame\nsame\nother\n")
    );
}

// ---- image --------------------------------------------------------------

#[tokio::test]
async fn image_attaches_each_kind_the_wires_take() {
    let desk = Desk::new();
    desk.put("p.png", b"\x89PNG\r\n\x1a\nrest");
    desk.put("j.jpg", [0xff_u8, 0xd8, 0xff, 0xe0]);
    desk.put("g.gif", b"GIF89a");
    desk.put("w.webp", b"RIFF\x00\x00\x00\x00WEBPVP8 ");
    let tools = desk.tools(true);

    let mut seen = Vec::new();
    for path in ["p.png", "j.jpg", "g.gif", "w.webp"] {
        let output = call(&tools, "image", json!({"path": path})).await;
        seen.push((
            output.text,
            output.image.map(|image| (image.kind, image.data.len())),
        ));
    }

    assert_eq!(
        seen,
        [
            (
                "Attached p.png (12 bytes, image/png) for your next step".to_owned(),
                Some((ImageKind::Png, 12))
            ),
            (
                "Attached j.jpg (4 bytes, image/jpeg) for your next step".to_owned(),
                Some((ImageKind::Jpeg, 4))
            ),
            (
                "Attached g.gif (6 bytes, image/gif) for your next step".to_owned(),
                Some((ImageKind::Gif, 6))
            ),
            (
                "Attached w.webp (16 bytes, image/webp) for your next step".to_owned(),
                Some((ImageKind::Webp, 16))
            ),
        ]
    );
}

#[tokio::test]
async fn image_refuses_a_wire_without_images_and_a_file_that_is_not_one() {
    let desk = Desk::new();
    desk.put("p.png", b"\x89PNG\r\n\x1a\n");
    desk.put("t.txt", "plain text");
    desk.put("riff.wav", b"RIFF\x00\x00\x00\x00WAVEfmt ");
    desk.put("empty.png", "");
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(IMAGE_MAX + 1, 0);
    desk.put("big.png", big);
    let blind = desk.tools(false);
    let tools = desk.tools(true);

    let no_wire = call(&blind, "image", json!({"path": "p.png"})).await;
    let text = call(&tools, "image", json!({"path": "t.txt"})).await;
    let wave = call(&tools, "image", json!({"path": "riff.wav"})).await;
    let empty = call(&tools, "image", json!({"path": "empty.png"})).await;
    let large = call(&tools, "image", json!({"path": "big.png"})).await;
    let missing = call(&tools, "image", json!({"path": "nowhere.png"})).await;
    let outside = call(&tools, "image", json!({"path": "../p.png"})).await;

    assert_eq!(
        no_wire.text,
        "[image_input_unavailable] this model's wire takes no image in a tool result"
    );
    assert_eq!(
        text.text,
        "[not_an_image] t.txt is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        wave.text,
        "[not_an_image] riff.wav is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        empty.text,
        "[not_an_image] empty.png is not a PNG, JPEG, GIF or WebP image"
    );
    assert_eq!(
        large.text,
        "[file_too_large] big.png is longer than 5242880 bytes"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(code(&outside), Some("path_not_allowed"));
    assert_eq!(large.image, None);
}

// ---- ls, find, grep -----------------------------------------------------

/// A small tree: files at two depths, a dotfile, `.git`, and a link.
fn tree(desk: &Desk) {
    desk.put("README.md", "read me\n");
    desk.put(".env", "KEY=1\n");
    desk.put("src/a.rs", "fn alpha() {}\n// TODO: tidy\n");
    desk.put("src/b.rs", "fn beta() {}\n");
    desk.put("src/deep/c.rs", "fn gamma() { todo!() }\n");
    desk.put("docs/guide.txt", "Caf\u{00e9} guide\nTODO later\n");
    desk.put(".git/HEAD", "TODO: never read\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(desk.path("src/a.rs"), desk.path("link.rs")).expect("linked");
}

#[tokio::test]
async fn ls_lists_sorted_with_directories_marked_and_stops_at_its_limit() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let root = call(&tools, "ls", json!({})).await;
    let src = call(&tools, "ls", json!({"path": "src"})).await;
    let dot = call(&tools, "ls", json!({"path": "."})).await;
    let limited = call(&tools, "ls", json!({"path": "src", "limit": 2})).await;

    let mut expected = vec![".env", ".git/", "README.md", "docs/", "src/"];
    if cfg!(unix) {
        expected.insert(4, "link.rs");
    }
    assert_eq!(root.text, expected.join("\n"));
    assert_eq!(dot.text, root.text);
    assert_eq!(src.text, "a.rs\nb.rs\ndeep/");
    assert_eq!(limited.text, format!("a.rs\nb.rs\n{LIMIT_REACHED}"));
}

#[tokio::test]
async fn ls_refuses_a_path_it_cannot_list() {
    let desk = Desk::new();
    desk.put("file.txt", "x");
    let tools = desk.tools(false);

    let missing = call(&tools, "ls", json!({"path": "nowhere"})).await;
    let outside = call(&tools, "ls", json!({"path": "../other"})).await;
    let dots = call(&tools, "ls", json!({"path": ".."})).await;
    let a_file = call(&tools, "ls", json!({"path": "file.txt"})).await;
    let empty = call(&tools, "ls", json!({"path": ""})).await;

    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(
        outside.text,
        format!("[path_not_allowed] ../other {LEAVES}")
    );
    assert_eq!(
        dots.text,
        "[invalid_arguments] path must name a file in the working directory"
    );
    assert_eq!(code(&a_file), Some("sandbox_unavailable"));
    assert_eq!(
        empty.text, "file.txt",
        "an empty path is the working directory"
    );
}

/// A directory with more entries than one listing carries is listed in
/// part, and says so.
#[tokio::test]
async fn ls_and_find_say_so_when_a_directory_holds_more_than_one_listing() {
    let desk = Desk::new();
    for number in 0..1001 {
        desk.put(&format!("many/f{number:04}.txt"), "");
    }
    let tools = desk.tools(false);

    let listed = call(&tools, "ls", json!({"path": "many", "limit": 2000})).await;
    let found = call(&tools, "find", json!({"pattern": "*.txt", "limit": 5000})).await;

    assert_eq!(
        listed.text.lines().count(),
        1001,
        "1000 entries and the note"
    );
    assert!(
        listed.text.ends_with(LIMIT_REACHED),
        "{}",
        &listed.text[listed.text.len() - 60..]
    );
    assert_eq!(found.text.lines().count(), 1001);
    assert!(
        found
            .text
            .ends_with("... the walk stopped early; narrow the path")
    );
}

#[tokio::test]
async fn find_matches_a_glob_below_the_path_and_skips_git_and_links() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let any_depth = call(&tools, "find", json!({"pattern": "*.rs"})).await;
    let rooted = call(&tools, "find", json!({"pattern": "src/*.rs"})).await;
    let globstar = call(&tools, "find", json!({"pattern": "src/**/*.rs"})).await;
    let one_char = call(&tools, "find", json!({"pattern": "?.rs", "path": "src"})).await;
    let star_star = call(&tools, "find", json!({"pattern": "**"})).await;
    let limited = call(&tools, "find", json!({"pattern": "**/*.rs", "limit": 1})).await;
    let none = call(&tools, "find", json!({"pattern": "*.py"})).await;
    let dots = call(&tools, "find", json!({"pattern": "(a).rs"})).await;

    assert_eq!(any_depth.text, "src/a.rs\nsrc/b.rs\nsrc/deep/c.rs");
    assert_eq!(rooted.text, "src/a.rs\nsrc/b.rs");
    assert_eq!(globstar.text, "src/a.rs\nsrc/b.rs\nsrc/deep/c.rs");
    assert_eq!(one_char.text, "a.rs\nb.rs\ndeep/c.rs");
    assert_eq!(
        star_star.text,
        ".env\nREADME.md\ndocs/guide.txt\nsrc/a.rs\nsrc/b.rs\nsrc/deep/c.rs"
    );
    assert_eq!(limited.text, format!("src/a.rs\n{LIMIT_REACHED}"));
    assert_eq!(none.text, "");
    assert_eq!(
        dots.text, "",
        "a glob's other characters match only themselves"
    );
}

#[tokio::test]
async fn find_refuses_a_path_it_cannot_walk() {
    let desk = Desk::new();
    desk.put("file.txt", "x");
    let tools = desk.tools(false);

    let missing = call(&tools, "find", json!({"pattern": "*", "path": "nowhere"})).await;
    let outside = call(&tools, "find", json!({"pattern": "*", "path": "../up"})).await;
    let no_pattern = call(&tools, "find", json!({"path": "."})).await;

    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(outside.text, format!("[path_not_allowed] ../up {LEAVES}"));
    assert_eq!(
        no_pattern.text,
        "[invalid_arguments] missing field `pattern`"
    );
}

#[tokio::test]
async fn grep_reads_back_each_matching_line_with_its_place() {
    let desk = Desk::new();
    tree(&desk);
    desk.put("bin.dat", [0xff_u8, b'T', b'O', b'D', b'O']);
    let tools = desk.tools(false);

    let regex = call(&tools, "grep", json!({"pattern": "fn (alpha|beta)"})).await;
    let literal = call(
        &tools,
        "grep",
        json!({"pattern": "todo!()", "literal": true}),
    )
    .await;
    let only_text = call(&tools, "grep", json!({"pattern": "TODO", "glob": "*.txt"})).await;
    let one_file = call(
        &tools,
        "grep",
        json!({"pattern": "TODO", "path": "src/a.rs"}),
    )
    .await;
    let below = call(&tools, "grep", json!({"pattern": "fn", "path": "src/deep"})).await;
    let accented = call(&tools, "grep", json!({"pattern": "Caf\u{00e9}"})).await;
    let limited = call(&tools, "grep", json!({"pattern": "fn", "limit": 1})).await;

    assert_eq!(
        regex.text,
        "src/a.rs:1: fn alpha() {}\nsrc/b.rs:1: fn beta() {}"
    );
    assert_eq!(literal.text, "src/deep/c.rs:1: fn gamma() { todo!() }");
    assert_eq!(only_text.text, "docs/guide.txt:2: TODO later");
    assert_eq!(one_file.text, "src/a.rs:2: // TODO: tidy");
    assert_eq!(
        below.text, "c.rs:1: fn gamma() { todo!() }",
        "relative to the path searched"
    );
    assert_eq!(accented.text, "docs/guide.txt:1: Caf\u{00e9} guide");
    assert_eq!(
        limited.text.lines().count(),
        2,
        "one line and the note: {}",
        limited.text
    );
    assert!(limited.text.ends_with(LIMIT_REACHED));
}

/// `ignore_case` matches a pattern in any case, as the tool's schema says.
#[tokio::test]
async fn grep_with_ignore_case_matches_any_case() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let output = call(
        &tools,
        "grep",
        json!({"pattern": "todo", "ignore_case": true, "glob": "*.txt"}),
    )
    .await;

    assert_eq!(output.text, "docs/guide.txt:2: TODO later");
}

/// The pattern is a regular expression, so a class such as `\\d` matches a
/// digit.
#[tokio::test]
async fn grep_reads_a_digit_class() {
    let desk = Desk::new();
    desk.put("n.txt", "none\nport 8080\n");
    let tools = desk.tools(false);

    let output = call(&tools, "grep", json!({"pattern": "\\d+"})).await;

    assert_eq!(output.text, "n.txt:2: port 8080");
}

#[tokio::test]
async fn grep_cuts_a_long_line_and_stops_at_the_output_budget() {
    let desk = Desk::new();
    let long = format!("hit {}", "\u{00e9}".repeat(600));
    desk.put("long.txt", format!("{long}\n").repeat(100));
    let tools = desk.tools(false);

    let output = call(&tools, "grep", json!({"pattern": "hit"})).await;

    let shown: String = long.chars().take(500).collect();
    let first = output.text.lines().next().expect("a line");
    assert_eq!(first, format!("long.txt:1: {shown}"));
    let lines = output.text.lines().count();
    assert!(
        lines < 100,
        "the 40,000-byte budget stops it first: {lines} lines"
    );
    assert!(output.text.ends_with(LIMIT_REACHED));
}

#[tokio::test]
async fn grep_refuses_a_pattern_or_path_it_cannot_use() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let bad_regex = call(&tools, "grep", json!({"pattern": "fn ("})).await;
    let outside = call(&tools, "grep", json!({"pattern": "x", "path": "../etc"})).await;
    let missing_file = call(
        &tools,
        "grep",
        json!({"pattern": "x", "path": "nowhere.rs"}),
    )
    .await;
    let empty = call(&tools, "grep", json!({"pattern": "nothing matches this"})).await;

    assert!(
        bad_regex
            .text
            .starts_with("[invalid_arguments] regex parse error"),
        "{}",
        bad_regex.text
    );
    assert_eq!(outside.text, format!("[path_not_allowed] ../etc {LEAVES}"));
    assert_eq!(
        missing_file.text, "",
        "a file that cannot be read holds no line"
    );
    assert_eq!(empty.text, "");
}

// ---- apply_patch --------------------------------------------------------

#[tokio::test]
async fn apply_patch_adds_updates_moves_and_deletes_in_one_patch() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\ntwo\nthree\n");
    desk.put("mv.txt", "m\n");
    desk.put("old.txt", "x\ny\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Add File: new.txt\n\
                 +hello\n\
                 +w\u{00f6}rld\n\
                 *** Update File: keep.txt\n\
                 @@\n \
                 one\n\
                 -two\n\
                 +2\n \
                 three\n\
                 *** Update File: mv.txt\n\
                 *** Move to: moved/mv.txt\n\
                 @@\n\
                 -m\n\
                 +n\n\
                 *** Delete File: old.txt\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nA new.txt\nM keep.txt\nM moved/mv.txt\nD old.txt\n\
         Lines changed:\n\"new.txt\" +2 -0\n\"keep.txt\" +1 -1\n\"moved/mv.txt\" +1 -1\n\"old.txt\" +0 -2"
    );
    assert_eq!(
        desk.text("new.txt").as_deref(),
        Some("hello\nw\u{00f6}rld\n")
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n2\nthree\n"));
    assert_eq!(desk.text("moved/mv.txt").as_deref(), Some("n\n"));
    assert_eq!(desk.text("mv.txt"), None);
    assert_eq!(desk.text("old.txt"), None);
}

/// Hunks on one file apply in order, each to what the one before left, and
/// a file one hunk touched twice is counted once.
#[tokio::test]
async fn apply_patch_applies_later_hunks_to_what_earlier_ones_left() {
    let desk = Desk::new();
    desk.put("f.txt", "a\nb\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: f.txt\n\
                 -a\n\
                 +A\n\
                 *** Update File: ./f.txt\n\
                 *** Move to: f.txt\n\
                 @@ A\n\
                 -b\n\
                 +B\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nM f.txt\nM ./f.txt\nLines changed:\n\"f.txt\" +1 -1\n\"./f.txt\" +1 -1"
    );
    assert_eq!(desk.text("f.txt").as_deref(), Some("A\nB\n"));
}

/// Codex reads a patch wrapped in a heredoc, which one model writes, and a
/// file's own line endings survive an update.
#[tokio::test]
async fn apply_patch_reads_a_heredoc_and_keeps_a_files_line_endings() {
    let desk = Desk::new();
    desk.put("crlf.txt", "a\r\nb\r\n");
    desk.put("cr.txt", "a\rb\r");
    let tools = desk.tools(false);
    let patch = "<<'EOF'\n\
                 *** Begin Patch\n\
                 *** Update File: crlf.txt\n\
                 @@\n \
                 a\n\
                 -b\n\
                 +B\n\
                 +C\n\
                 *** Update File: cr.txt\n\
                 @@\n\
                 +c\n\
                 *** End Patch\n\
                 \n\
                 *** End Patch\n\
                 EOF";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nM crlf.txt\nM cr.txt\nLines changed:\n\"crlf.txt\" +2 -1\n\"cr.txt\" +1 -0"
    );
    assert_eq!(desk.text("crlf.txt").as_deref(), Some("a\r\nB\r\nC\r\n"));
    assert_eq!(desk.text("cr.txt").as_deref(), Some("a\rb\rc\r"));
}

/// An update hunk may start its lines without an `@@` marker.
#[tokio::test]
async fn apply_patch_takes_an_update_whose_lines_need_no_marker() {
    let desk = Desk::new();
    desk.put("ctx.txt", "a\nb\n");
    desk.put("minus.txt", "a\nb\n");
    desk.put("plus.txt", "a\n");
    desk.put("blank.txt", "\nb\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: ctx.txt\n \
                 a\n\
                 -b\n\
                 *** Update File: minus.txt\n\
                 -a\n\
                 *** Update File: plus.txt\n\
                 +b\n\
                 *** Update File: blank.txt\n\
                 \n\
                 -b\n\
                 +B\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(code(&output), None, "{}", output.text);
    assert_eq!(desk.text("ctx.txt").as_deref(), Some("a\n"));
    assert_eq!(desk.text("minus.txt").as_deref(), Some("b\n"));
    assert_eq!(desk.text("plus.txt").as_deref(), Some("a\nb\n"));
    assert_eq!(desk.text("blank.txt").as_deref(), Some("\nB\n"));
}

/// The patch around a body of hunks.
fn wrapped(body: &str) -> String {
    format!("*** Begin Patch\n{body}\n*** End Patch")
}

/// A patch that does not parse in Codex's grammar is refused in its words,
/// and changes no file.
#[tokio::test]
async fn apply_patch_refuses_a_patch_that_does_not_parse() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\n");
    let tools = desk.tools(false);

    let no_begin = call(
        &tools,
        "apply_patch",
        json!({"patch": "hello\n*** End Patch"}),
    )
    .await;
    let no_end = call(
        &tools,
        "apply_patch",
        json!({"patch": "*** Begin Patch\n*** Add File: a\n+x"}),
    )
    .await;
    let one_line = call(&tools, "apply_patch", json!({"patch": "garbage"})).await;
    let empty = call(&tools, "apply_patch", json!({"patch": ""})).await;
    let no_hunks = call(
        &tools,
        "apply_patch",
        json!({"patch": "*** Begin Patch\n*** End Patch"}),
    )
    .await;
    let blank_line = call(&tools, "apply_patch", json!({"patch": wrapped("")})).await;
    let bad_line = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n@@\n?bad")}),
    )
    .await;
    let no_marker = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n-one\n+1\nstray")}),
    )
    .await;

    assert_eq!(
        no_begin.text,
        "[patch_invalid] invalid patch: The first line of the patch must be '*** Begin Patch'"
    );
    assert_eq!(
        no_end.text,
        "[patch_invalid] invalid patch: The last line of the patch must be '*** End Patch'"
    );
    assert_eq!(
        one_line.text,
        "[patch_invalid] invalid patch: The first line of the patch must be '*** Begin Patch'"
    );
    assert_eq!(
        empty.text,
        "[patch_invalid] invalid patch: The last line of the patch must be '*** End Patch'"
    );
    assert_eq!(no_hunks.text, "[patch_invalid] the patch changes no file");
    assert_eq!(
        blank_line.text,
        "[patch_invalid] invalid hunk at line 2, '' is not a valid hunk header. Valid hunk \
         headers: '*** Add File: {path}', '*** Delete File: {path}', '*** Update File: {path}'"
    );
    assert_eq!(
        bad_line.text,
        "[patch_invalid] invalid hunk at line 4, Unexpected line found in update hunk: '?bad'. \
         Every line should start with ' ' (context line), '+' (added line), or '-' (removed line)"
    );
    assert_eq!(
        no_marker.text,
        "[patch_invalid] invalid hunk at line 5, Expected update hunk to start with a @@ context \
         marker, got: 'stray'"
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n"));
    assert_eq!(desk.text("a"), None);
}

/// A patch whose hunk does not apply, or names a path it may not touch,
/// changes no file, even one an earlier hunk would have added.
#[tokio::test]
async fn apply_patch_refuses_a_hunk_that_does_not_apply_and_changes_nothing() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\n");
    desk.put("old.txt", "x\n");
    let tools = desk.tools(false);

    let unmatched = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Add File: added.txt\n+new\n*** Update File: keep.txt\n-zzz\n+y")}),
    )
    .await;
    let after_delete = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Delete File: old.txt\n*** Update File: old.txt\n-x\n+y")}),
    )
    .await;
    let after_move = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: old.txt\n*** Move to: new.txt\n-x\n+y\n*** Delete File: old.txt")}),
    )
    .await;
    let missing = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Delete File: nowhere.txt")}),
    )
    .await;
    let outside = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Add File: ../x.txt\n+x")}),
    )
    .await;
    let move_outside = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n*** Move to: /tmp/x.txt\n-one\n+1")}),
    )
    .await;

    assert_eq!(
        unmatched.text,
        "[patch_invalid] Failed to find expected lines in keep.txt:\nzzz"
    );
    assert_eq!(
        after_delete.text,
        "[file_not_found] old.txt was moved or deleted earlier in this patch"
    );
    assert_eq!(
        after_move.text,
        "[file_not_found] old.txt was moved or deleted earlier in this patch"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(
        outside.text,
        format!("[path_not_allowed] ../x.txt {LEAVES}")
    );
    assert_eq!(
        move_outside.text,
        format!("[path_not_allowed] /tmp/x.txt {LEAVES}")
    );
    assert_eq!(
        desk.text("added.txt"),
        None,
        "the add before the bad hunk never landed"
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n"));
    assert_eq!(desk.text("old.txt").as_deref(), Some("x\n"));
    assert_eq!(desk.text("new.txt"), None);
}

/// A hunk whose result would be longer than one read carries is refused as
/// it is planned, and nothing lands.
#[tokio::test]
async fn apply_patch_refuses_a_result_longer_than_one_read() {
    let desk = Desk::new();
    desk.put("near.txt", format!("{}\n", "z".repeat(ONE_READ - 2)));
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n*** Update File: near.txt\n+more\n*** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "[file_too_large] near.txt is longer than 8388608 bytes"
    );
    assert_eq!(
        desk.text("near.txt").map(|text| text.len()),
        Some(ONE_READ - 1)
    );
}

/// An update to a file that cannot be read whole as text is refused.
#[tokio::test]
async fn apply_patch_refuses_an_update_to_a_file_that_is_not_text() {
    let desk = Desk::new();
    desk.put("bin.dat", [0xff_u8, b'\n']);
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n*** Update File: bin.dat\n+x\n*** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "[invalid_arguments] bin.dat is not text, so it cannot be edited"
    );
}

/// Two spellings of one file, a link and its target, are planned apart and
/// land one after the other, each applied to the file as it is then.
#[cfg(unix)]
#[tokio::test]
async fn apply_patch_through_a_link_lands_each_hunk_on_the_file_as_it_is() {
    let desk = Desk::new();
    desk.put("real.txt", "a\nb\n");
    std::os::unix::fs::symlink(desk.path("real.txt"), desk.path("alias.txt")).expect("linked");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: real.txt\n\
                 -a\n\
                 +A\n\
                 *** Update File: alias.txt\n\
                 -b\n\
                 +B\n\
                 *** End Patch";
    let stale = "*** Begin Patch\n\
                 *** Update File: real.txt\n\
                 -A\n\
                 +a\n\
                 *** Update File: alias.txt\n\
                 -A\n\
                 +x\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;
    let landed = desk.text("real.txt");
    let half = call(&tools, "apply_patch", json!({"patch": stale})).await;

    assert_eq!(code(&output), None, "{}", output.text);
    assert_eq!(landed.as_deref(), Some("A\nB\n"));
    assert_eq!(
        half.text, "[patch_invalid] Failed to find expected lines in alias.txt:\nA",
        "the second hunk no longer applies once the first landed"
    );
    assert_eq!(
        desk.text("real.txt").as_deref(),
        Some("a\nB\n"),
        "the hunk before it stays landed"
    );
}

// ---- tool_search --------------------------------------------------------

#[tokio::test]
async fn tool_search_loads_the_deferred_tools_that_best_match() {
    let parameters = json!({"type": "object"});
    let specs = [
        ("github_issue_create", "Create a GitHub issue."),
        ("github_issue_close", "Close a GitHub issue."),
        ("jira_ticket_create", "Create a Jira ticket."),
        ("cafe_menu", "Read the caf\u{00e9}'s menu."),
    ]
    .map(|(name, description)| ToolSpec {
        name,
        description,
        parameters: &parameters,
    });
    let deferred = Arc::new(Deferred::new(specs));
    let tools: Tools<ContextTool<'_>> = Tools::builder()
        .with_tool(Typed::new(ToolSearch::new(Arc::clone(&deferred))))
        .build(&ToolsConfig::new(["tool_search"]).expect("a name"))
        .expect("needs no backend");

    let best = call(
        &tools,
        "tool_search",
        json!({"query": "create github issue", "limit": 2}),
    )
    .await;
    let accented = call(&tools, "tool_search", json!({"query": "CAF\u{00c9}"})).await;
    let nothing = call(&tools, "tool_search", json!({"query": "deploy"})).await;
    let punctuation = call(&tools, "tool_search", json!({"query": "!!! ??"})).await;
    let empty = call(&tools, "tool_search", json!({"query": ""})).await;
    let capped = call(&tools, "tool_search", json!({"query": "a", "limit": 1000})).await;

    assert_eq!(
        best.text,
        "github_issue_create: Create a GitHub issue.\ngithub_issue_close: Close a GitHub issue.\n\
         These tools are offered from your next step."
    );
    assert_eq!(
        accented.text,
        "cafe_menu: Read the caf\u{00e9}'s menu.\nThese tools are offered from your next step."
    );
    assert_eq!(nothing.text, "no deferred tool matches deploy");
    assert_eq!(punctuation.text, "no deferred tool matches !!! ??");
    assert_eq!(empty.text, "no deferred tool matches ");
    assert_eq!(
        capped.text.lines().count(),
        5,
        "every tool holds `a`, and the note: {}",
        capped.text
    );
    assert!(deferred.is_loaded("cafe_menu"));
    assert_eq!(
        deferred.loaded(),
        [
            "cafe_menu",
            "github_issue_close",
            "github_issue_create",
            "jira_ticket_create"
        ]
    );
}

#[tokio::test]
async fn tool_search_before_any_search_has_loaded_nothing() {
    let deferred = Deferred::new(std::iter::empty::<ToolSpec<'_>>());

    assert!(!deferred.is_loaded("anything"));
    assert_eq!(deferred.loaded(), Vec::<String>::new());
}

/// A write the environment refuses as the patch lands is reported, and the
/// hunks before it stay landed.
#[tokio::test]
async fn apply_patch_reports_a_write_refused_as_it_lands_and_keeps_what_landed() {
    let desk = Desk::new();
    std::fs::create_dir(desk.path("taken")).expect("made");
    let tools = desk.tools(false);
    let patch = wrapped("*** Add File: first.txt\n+one\n*** Add File: taken\n+two");

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        code(&output),
        Some("sandbox_unavailable"),
        "{}",
        output.text
    );
    assert_eq!(desk.text("first.txt").as_deref(), Some("one\n"));
    assert!(desk.path("taken").is_dir(), "the directory stays");
}

/// An entry that is neither a file, a directory nor a link, such as a
/// socket, is listed by its name and never walked into or searched.
#[cfg(unix)]
#[tokio::test]
async fn ls_names_a_socket_and_find_and_grep_pass_over_it() {
    let desk = Desk::new();
    desk.put("plain.txt", "sock\n");
    let _listener =
        std::os::unix::net::UnixListener::bind(desk.path("sock")).expect("a socket file");
    let tools = desk.tools(false);

    let listed = call(&tools, "ls", json!({})).await;
    let found = call(&tools, "find", json!({"pattern": "*"})).await;
    let grepped = call(&tools, "grep", json!({"pattern": "sock"})).await;

    assert_eq!(listed.text, "plain.txt\nsock");
    assert_eq!(found.text, "plain.txt");
    assert_eq!(grepped.text, "plain.txt:1: sock");
}
