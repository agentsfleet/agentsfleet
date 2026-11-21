//! The thread read's pure half: flattening and the page shape.
//!
//! The requests themselves are proved over a socket in `tests/slack_replies.rs`.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use super::{Page, Posted, Unavailable, flatten};

/// One posted message, parsed from Slack's own rendering.
fn posted(json: &str) -> Posted {
    serde_json::from_str(json).expect("the fixture is a Slack message")
}

/// A CI announcement whose run link sits only in an
/// attachment's `title_link` still carries that link after flattening.
#[test]
fn attachment_links_survive_flattening() {
    let announcement = posted(
        r#"{"ts":"1700000000.000100","bot_id":"B0GITHUB","text":"",
            "attachments":[{"fallback":"CI failed","color":"danger",
              "title":"ci / build failed on main",
              "title_link":"https://github.com/acme/widgets/actions/runs/123",
              "fields":[{"title":"Commit","value":"abc1234","short":true}]}]}"#,
    );
    let message = flatten(announcement);
    assert!(
        message
            .text
            .contains("https://github.com/acme/widgets/actions/runs/123"),
        "{}",
        message.text
    );
    assert!(message.text.contains("ci / build failed on main"));
    assert!(message.text.contains("abc1234"), "a field's value is shown");
    assert!(
        !message.text.contains("CI failed"),
        "`fallback` repeats what is shown and is left out"
    );
    assert_eq!(message.author, "B0GITHUB", "a bot is named by its bot id");
}

/// Block text is read at any depth, and text Slack repeats in its blocks is
/// kept once.
#[test]
fn block_text_is_read_once_at_any_depth() {
    let message = flatten(posted(
        r#"{"ts":"1","user":"U01","text":"deploy is stuck",
            "blocks":[
              {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"deploy is stuck"},
                {"type":"link","url":"https://status.example.test/incident/9"}]}]},
              {"type":"section","text":{"type":"mrkdwn","text":"see the status page"}}]}"#,
    ));
    assert_eq!(
        message.text,
        "deploy is stuck\nhttps://status.example.test/incident/9\nsee the status page"
    );
    assert_eq!(message.author, "U01");
}

/// A message naming neither a person nor a bot has an empty author, and one
/// carrying nothing shown has empty text.
#[test]
fn a_message_with_nothing_shown_flattens_to_empty() {
    let message = flatten(posted(r#"{"ts":"1","attachments":[{"color":"good"}]}"#));
    assert_eq!(message.author, "");
    assert_eq!(message.text, "");
}

/// A 200 that is JSON but not Slack's answer reads as not ok, never as an
/// empty thread the fleet would take for a quiet one.
#[test]
fn a_body_that_is_not_slacks_answer_is_not_ok() {
    let page: Page = serde_json::from_str(r#"{"status":"healthy"}"#).expect("it is JSON");
    assert!(!page.ok);
    assert!(page.messages.is_empty());
    assert!(!page.has_more);
    assert_eq!(page.response_metadata.next_cursor, "");
}

/// Every reason has its own spelling, so the fleet and the operator can tell
/// a slow Slack from a refusing one.
#[test]
fn every_reason_is_spelled_apart() {
    let reasons = [
        Unavailable::Timeout,
        Unavailable::Refused,
        Unavailable::Status,
        Unavailable::Unreachable,
        Unavailable::Unreadable,
    ];
    let mut spelled: Vec<&str> = reasons.iter().map(|reason| reason.as_str()).collect();
    spelled.sort_unstable();
    spelled.dedup();
    assert_eq!(spelled.len(), reasons.len(), "{spelled:?}");
}
