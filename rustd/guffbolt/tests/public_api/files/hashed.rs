//! `file_read_hashed` tags lines, and `file_edit_hashed` edits by those tags.

use super::*;

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
