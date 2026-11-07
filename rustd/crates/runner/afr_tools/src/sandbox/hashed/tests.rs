#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use serde_json::json;

use super::{Target, tag};
use crate::catalog::{FILE_EDIT_HASHED, FILE_READ_HASHED};
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{Live, call_in, hosted, offered};

/// The argument names the calls spell.
pub(super) const PATH: &str = "path";
pub(super) const TARGET: &str = "target";
pub(super) const END_TARGET: &str = "end_target";
pub(super) const NEW_TEXT: &str = "new_text";

/// The file the edits work on, and what it starts as.
pub(super) const GREEK: &str = "greek.txt";
pub(super) const THREE_LINES: &str = "alpha\nbeta\ngamma\n";

/// The tag of line `number` in a `file_read_hashed` answer.
pub(super) fn tag_of(read: &ToolOutput, number: usize) -> String {
    read.text
        .lines()
        .nth(number - 1)
        .and_then(|line| line.split('|').next())
        .unwrap()
        .to_owned()
}

/// Reads `GREEK` tagged, then edits it with `target`, `end_target` and
/// `new_text`; the read and the edit.
async fn read_then_edit(
    live: &Live,
    target: &str,
    end_target: Option<&str>,
    new_text: &str,
    between: impl FnOnce(&Live),
) -> (ToolOutput, ToolOutput) {
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
    between(live);
    let target = target.replace("{L2}", &tag_of(&read, 2));
    let end_target = end_target.map(|end| end.replace("{L3}", &tag_of(&read, 3)));
    let edit = call_in(
        offered(&selection, &FILE_EDIT_HASHED),
        &live.client,
        &lease,
        json!({PATH: GREEK, TARGET: target, END_TARGET: end_target, NEW_TEXT: new_text}),
    )
    .await;
    (read, edit)
}

#[tokio::test]
async fn test_hashed_edit_refuses_stale_hash() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (read, stale) = read_then_edit(&live, "{L2}", None, "BETA", |live| {
        std::fs::write(live.root.join(GREEK), "alpha\nbeta!\ngamma\n").unwrap();
    })
    .await;
    let (_reread, fresh) = read_then_edit(&live, "{L2}", None, "BETA", |_live| {}).await;

    assert_eq!(read.error_code, None, "{read:?}");
    assert_eq!(
        stale.error_code,
        Some(ToolErrorCode::HashMismatch),
        "{stale:?}"
    );
    assert!(stale.text.contains("no longer matches"), "{}", stale.text);
    assert_eq!(fresh.error_code, None, "{fresh:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha\nBETA\ngamma\n",
        "only the fresh edit landed"
    );
    live.stop().await;
}

#[tokio::test]
async fn a_read_tags_every_line_with_its_number_and_hash() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_READ_HASHED.name()]).unwrap();
    let lease = Lease::default();

    let read = call_in(
        offered(&selection, &FILE_READ_HASHED),
        &live.client,
        &lease,
        json!({PATH: GREEK}),
    )
    .await;

    let expected = format!(
        "L1:{}|alpha\nL2:{}|beta\nL3:{}|gamma\nL4:{}|\n",
        tag("", "alpha"),
        tag("alpha", "beta"),
        tag("beta", "gamma"),
        tag("gamma", "")
    );
    assert_eq!(read.text, expected);
    live.stop().await;
}

/// Lines added above the target move it; its tag is found within the radius
/// and the edit lands where the line is now.
#[tokio::test]
async fn a_tag_is_found_after_lines_moved_above_it() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (_read, edit) = read_then_edit(&live, "{L2}", None, "BETA", |live| {
        std::fs::write(live.root.join(GREEK), format!("zero\nnil\n{THREE_LINES}")).unwrap();
    })
    .await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert!(edit.text.contains("was line 4"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "zero\nnil\nalpha\nBETA\ngamma\n"
    );
    live.stop().await;
}

#[tokio::test]
async fn a_range_is_replaced_from_target_to_end_target() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (_read, edit) = read_then_edit(&live, "{L2}", Some("{L3}"), "both", |_live| {}).await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert!(edit.text.contains("2 lines replaced"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha\nboth",
        "nothing but the file's end follows, so no newline is added"
    );
    live.stop().await;
}

#[tokio::test]
async fn an_end_target_before_the_target_or_past_the_end_is_a_mismatch() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (read, before) = read_then_edit(&live, "{L2}", Some("L1:000"), "x", |_live| {}).await;
    let past = format!("L9:{}", tag_of(&read, 3).split(':').nth(1).unwrap());
    let (_read, beyond) = read_then_edit(&live, "{L2}", Some(&past), "x", |_live| {}).await;

    for refused in [&before, &beyond] {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::HashMismatch),
            "{refused:?}"
        );
    }
    assert!(before.text.contains("comes before"), "{}", before.text);
    assert!(beyond.text.contains("past the end"), "{}", beyond.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        THREE_LINES
    );
    live.stop().await;
}

#[tokio::test]
async fn a_tag_matching_twice_nearby_is_refused() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), "x\nx\nx\n").unwrap();

    let (read, edit) = read_then_edit(&live, "{L2}", None, "y", |_live| {}).await;

    let hash_of = |number| tag_of(&read, number).split(':').nth(1).map(str::to_owned);
    assert_eq!(hash_of(2), hash_of(3), "the two inner lines hash alike");
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::HashMismatch),
        "{edit:?}"
    );
    assert!(edit.text.contains("more than once"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "x\nx\nx\n"
    );
    live.stop().await;
}

#[tokio::test]
async fn a_tag_that_does_not_parse_or_names_line_zero_is_invalid() {
    for given in ["10:abc", "L10abc", "L10:ab", "L0:abc", "Lten:abc", ""] {
        let refused = Target::parse(given).unwrap_err();

        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{given:?}"
        );
        assert!(refused.text.contains("is not a tag"), "{}", refused.text);
    }
    let parsed = Target::parse("L12:abc").unwrap();
    assert_eq!((parsed.line, parsed.hash), (12, "abc"));
}

#[tokio::test]
async fn a_target_past_the_end_of_the_file_is_a_mismatch() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (read, edit) = read_then_edit(&live, "{L2}", None, "x", |live| {
        std::fs::write(live.root.join(GREEK), "alpha").unwrap();
    })
    .await;

    assert_eq!(read.error_code, None, "{read:?}");
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::HashMismatch),
        "{edit:?}"
    );
    assert!(edit.text.contains("past the end"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha"
    );
    live.stop().await;
}

/// The file was reordered under the model: the end tag's line now sits
/// above the start tag's, so the range is refused rather than inverted.
#[tokio::test]
async fn an_end_target_found_before_the_target_is_refused() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), "p\nq\nr\ns\n").unwrap();

    let (_read, edit) = read_then_edit(&live, "{L2}", Some("{L3}"), "x", |live| {
        std::fs::write(live.root.join(GREEK), "q\nr\nz\np\nq\n").unwrap();
    })
    .await;

    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::HashMismatch),
        "{edit:?}"
    );
    assert!(edit.text.contains("was found before"), "{}", edit.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "q\nr\nz\np\nq\n"
    );
    live.stop().await;
}

#[tokio::test]
async fn an_empty_new_text_removes_the_line() {
    let live = Live::start().await;
    std::fs::write(live.root.join(GREEK), THREE_LINES).unwrap();

    let (_read, edit) = read_then_edit(&live, "{L2}", None, "", |_live| {}).await;

    assert_eq!(edit.error_code, None, "{edit:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(GREEK)).unwrap(),
        "alpha\ngamma\n"
    );
    live.stop().await;
}
