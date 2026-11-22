//! `file_write` and `file_edit` on disk.

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

/// `file_edit`'s arguments: `edits`, each replacing `old` with `new`.
fn edits(path: &str, pairs: &[(&str, &str)]) -> Value {
    let edits: Vec<Value> = pairs
        .iter()
        .map(|(old, new)| json!({"old_text": old, "new_text": new}))
        .collect();
    json!({"path": path, "edits": edits})
}

/// Several edits land in one write, each where the file held its text, given
/// in any order, and the answer counts them and the lines they changed.
#[tokio::test]
async fn file_edit_applies_several_edits_in_one_write_and_counts_the_change() {
    let desk = Desk::new();
    desk.put("e.txt", "keep\nalpha\nmiddle\nbeta\nkeep\n");
    let tools = desk.tools(false);

    let output = call(
        &tools,
        "file_edit",
        edits(
            "e.txt",
            &[("beta", "\u{00e9}t\u{00e9}"), ("alpha", "ALPHA")],
        ),
    )
    .await;
    let one = call(&tools, "file_edit", edits("e.txt", &[("middle", "centre")])).await;

    assert_eq!(
        output.text,
        "Applied 2 edits to e.txt\nLines changed:\n\"e.txt\" +2 -2"
    );
    assert_eq!(
        one.text,
        "Applied 1 edit to e.txt\nLines changed:\n\"e.txt\" +1 -1"
    );
    assert_eq!(
        desk.text("e.txt").as_deref(),
        Some("keep\nALPHA\ncentre\n\u{00e9}t\u{00e9}\nkeep\n")
    );
}

/// Text the file holds more than once, overlapping itself too, is refused
/// with the count, and the file stays as it was.
#[tokio::test]
async fn file_edit_refuses_text_the_file_holds_more_than_once() {
    let desk = Desk::new();
    desk.put("e.txt", "x and x\naaa\n");
    let tools = desk.tools(false);

    let twice = call(&tools, "file_edit", edits("e.txt", &[("x", "y")])).await;
    let overlapping = call(&tools, "file_edit", edits("e.txt", &[("aa", "b")])).await;

    assert_eq!(code(&twice), Some("text_not_unique"));
    assert_eq!(
        twice.text,
        "[text_not_unique] edits[0].old_text occurs 2 times in e.txt; quote more of the text \
         around it so it occurs once"
    );
    assert_eq!(code(&overlapping), Some("text_not_unique"));
    assert!(
        overlapping.text.contains("occurs 2 times"),
        "{}",
        overlapping.text
    );
    assert_eq!(desk.text("e.txt").as_deref(), Some("x and x\naaa\n"));
}

/// One edit that cannot land refuses the whole call: the edits before it are
/// not written, an edit never matches text an earlier one wrote, and two
/// edits over the same text overlap.
#[tokio::test]
async fn file_edit_writes_nothing_when_any_edit_fails() {
    let desk = Desk::new();
    desk.put("e.txt", "alpha\nbeta\n");
    let tools = desk.tools(false);

    let absent = call(
        &tools,
        "file_edit",
        edits("e.txt", &[("alpha", "A"), ("omega", "O")]),
    )
    .await;
    let ambiguous = call(
        &tools,
        "file_edit",
        edits("e.txt", &[("beta", "B"), ("a", "_")]),
    )
    .await;
    let chained = call(
        &tools,
        "file_edit",
        edits("e.txt", &[("alpha", "gamma"), ("gamma", "delta")]),
    )
    .await;
    let overlapping = call(
        &tools,
        "file_edit",
        edits("e.txt", &[("beta", "B"), ("alpha\nbe", "A")]),
    )
    .await;

    assert_eq!(
        absent.text,
        "[text_not_found] e.txt does not contain edits[1].old_text"
    );
    assert_eq!(code(&ambiguous), Some("text_not_unique"));
    assert_eq!(
        chained.text,
        "[text_not_found] e.txt does not contain edits[1].old_text"
    );
    assert_eq!(
        overlapping.text,
        "[invalid_arguments] edits[0] and edits[1] overlap in e.txt; merge them into one edit"
    );
    assert_eq!(desk.text("e.txt").as_deref(), Some("alpha\nbeta\n"));
}

/// Each way an edit is refused leaves the file as it was.
#[tokio::test]
async fn file_edit_refuses_what_it_cannot_edit_and_leaves_the_file() {
    let desk = Desk::new();
    desk.put("e.txt", "alpha\n");
    desk.put("bin.dat", [0xff_u8, 0x00, 0xfe]);
    desk.put("big.txt", "y".repeat(ONE_READ + 1));
    let tools = desk.tools(false);
    let edit = |path: &str, old: &str| edits(path, &[(old, "z")]);

    let none = call(&tools, "file_edit", edits("e.txt", &[])).await;
    let empty = call(&tools, "file_edit", edit("e.txt", "")).await;
    let absent = call(&tools, "file_edit", edit("e.txt", "omega")).await;
    let binary = call(&tools, "file_edit", edit("bin.dat", "a")).await;
    let big = call(&tools, "file_edit", edit("big.txt", "y")).await;
    let missing = call(&tools, "file_edit", edit("nowhere.txt", "a")).await;
    let unknown = call(
        &tools,
        "file_edit",
        json!({"path": "e.txt", "edits": [{"old_text": "a", "new_txt": "z"}]}),
    )
    .await;
    let flat = call(
        &tools,
        "file_edit",
        json!({"path": "e.txt", "old_text": "a", "new_text": "z"}),
    )
    .await;

    assert_eq!(
        none.text,
        "[invalid_arguments] edits must hold at least one edit"
    );
    assert_eq!(
        empty.text,
        "[invalid_arguments] edits[0].old_text must not be empty"
    );
    assert_eq!(
        absent.text,
        "[text_not_found] e.txt does not contain edits[0].old_text"
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
    for refused in [&unknown, &flat] {
        assert!(
            refused
                .text
                .starts_with("[invalid_arguments] unknown field"),
            "{}",
            refused.text
        );
    }
    assert_eq!(desk.text("e.txt").as_deref(), Some("alpha\n"));
}
