//! `file_write`, `file_append`, `file_delete` and `file_edit` on disk.

use super::*;

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
