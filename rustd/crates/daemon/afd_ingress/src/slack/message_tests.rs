//! Composing the fleet's message from the question and the thread.

use afd_connector::slack::{Message, Replies, Unavailable};

use super::{THREAD_HEADING, THREAD_UNAVAILABLE, compose};

/// The mention's own id, which the thread holds and the fleet is not told.
const MENTION_TS: &str = "1700000000.999999";

fn said(ts: &str, author: &str, text: &str) -> Message {
    Message {
        ts: ts.to_owned(),
        author: author.to_owned(),
        text: text.to_owned(),
    }
}

/// A thread read whole.
const fn thread(messages: Vec<Message>) -> Replies {
    Replies { messages }
}

/// The question comes first, then the thread under its heading, one item per
/// message with the lines after its first indented, and the mention itself
/// left out.
#[test]
fn the_thread_follows_the_question_under_its_heading() {
    let read = Ok(thread(vec![
        said(
            "1",
            "B0GITHUB",
            "ci / build failed\nhttps://example.test/runs/123",
        ),
        said("2", "U01", "looking"),
        said(MENTION_TS, "U02", "<@UBOT> why did it fail?"),
    ]));
    let composed = compose("why did it fail?", MENTION_TS, &read);
    assert_eq!(
        composed.message,
        format!(
            "why did it fail?\n\n{THREAD_HEADING}\n- B0GITHUB: ci / build failed\n  \
             https://example.test/runs/123\n- U01: looking"
        )
    );
    assert!(composed.thread.fetched);
    assert_eq!(composed.thread.count, 2);
}

/// A thread holding only the mention tells nothing and adds no heading.
#[test]
fn a_thread_of_only_the_mention_adds_nothing() {
    let read = Ok(thread(vec![said(MENTION_TS, "U02", "<@UBOT> status?")]));
    let composed = compose("status?", MENTION_TS, &read);
    assert_eq!(composed.message, "status?");
    assert!(composed.thread.fetched);
    assert_eq!(composed.thread.count, 0);
}

/// A failed read leaves one line naming why, and says it was not read.
#[test]
fn a_failed_read_names_its_reason() {
    for reason in [
        Unavailable::Timeout,
        Unavailable::Refused,
        Unavailable::Status,
    ] {
        let composed = compose("why?", MENTION_TS, &Err(reason));
        assert_eq!(
            composed.message,
            format!("why?\n\n{THREAD_UNAVAILABLE}{}", reason.as_str())
        );
        assert!(!composed.thread.fetched);
        assert_eq!(composed.thread.count, 0);
    }
}

/// A message Slack named nobody for is told as `unknown`.
#[test]
fn an_unnamed_author_is_told_as_unknown() {
    let composed = compose(
        "why?",
        MENTION_TS,
        &Ok(thread(vec![said("1", "", "hello")])),
    );
    assert!(
        composed.message.ends_with("- unknown: hello"),
        "{}",
        composed.message
    );
}
