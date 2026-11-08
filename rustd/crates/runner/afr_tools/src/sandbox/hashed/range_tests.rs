#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! Edits that name a range by its start and end tags.

use serde_json::json;

use super::tests::{END_TARGET, GREEK, NEW_TEXT, PATH, TARGET, THREE_LINES, tag_of};
use crate::catalog::{FILE_EDIT_HASHED, FILE_READ_HASHED};
use crate::lease::Lease;
use crate::runtime::ToolOutput;
use crate::testing::{Live, call_in, hosted, offered};

/// `given` with every `{L<n>}` replaced by line n's tag in `read`.
fn with_tags(given: &str, read: &ToolOutput) -> String {
    (1..=read.text.lines().count()).fold(given.to_owned(), |text, number| {
        text.replace(&format!("{{L{number}}}"), &tag_of(read, number))
    })
}

/// Reads `GREEK` tagged, then edits it; `{L<n>}` in a target stands for
/// line n's tag as read.
async fn edit_by_tags(
    live: &Live,
    target: &str,
    end_target: Option<&str>,
    new_text: &str,
) -> ToolOutput {
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ_HASHED.name(), FILE_EDIT_HASHED.name()])
        .unwrap();
    let lease = Lease::default();
    let read = call_in(
        offered(&selection, &FILE_READ_HASHED),
        &live.client,
        &lease,
        json!({PATH: GREEK}),
    )
    .await;
    let target = with_tags(target, &read);
    let end_target = end_target.map(|end| with_tags(end, &read));
    call_in(
        offered(&selection, &FILE_EDIT_HASHED),
        &live.client,
        &lease,
        json!({PATH: GREEK, TARGET: target, END_TARGET: end_target, NEW_TEXT: new_text}),
    )
    .await
}

#[tokio::test]
async fn a_range_whose_end_is_its_start_replaces_that_one_line() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let edit = edit_by_tags(&live, "{L2}", Some("{L2}"), "BETA").await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert!(edit.text.contains("1 lines replaced"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha\nBETA\ngamma\n"
    );
    live.stop().await;
}

/// The end tag is looked for near where the start landed, so a range deep
/// in a long file is found at its own lines and nowhere else.
#[tokio::test]
async fn a_range_deep_in_a_long_file_is_found_at_its_own_lines() {
    let live = Live::start().await;
    let long = (1..=200)
        .map(|number| format!("line {number}\n"))
        .collect::<Vec<String>>()
        .concat();
    std::fs::write(live.root.join(GREEK), &long).unwrap();

    let edit = edit_by_tags(&live, "{L60}", Some("{L62}"), "mid").await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert!(edit.text.contains("3 lines replaced"), "{}", edit.text);
    let edited = std::fs::read_to_string(live.root.join(GREEK)).unwrap();
    assert_eq!(edited.lines().count(), 198);
    assert_eq!(edited.lines().nth(59), Some("mid"));
    assert_eq!(edited.lines().nth(60), Some("line 63"));
    live.stop().await;
}

#[tokio::test]
async fn the_last_tagged_line_can_be_edited() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let edit = edit_by_tags(&live, "{L4}", None, "delta").await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha\nbeta\ngamma\ndelta"
    );
    live.stop().await;
}
