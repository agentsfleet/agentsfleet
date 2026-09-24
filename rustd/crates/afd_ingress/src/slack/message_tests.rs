//! Composing the fleet's message from the question and the thread.

use afd_connector::slack::{Message, Replies, Unavailable};

use super::{MESSAGE_CAP, THREAD_CAP, THREAD_HEADING, THREAD_UNAVAILABLE, compose};

/// The mention's own id, which the thread holds and the fleet is not told.
const MENTION_TS: &str = "1700000000.999999";

fn said(ts: &str, author: &str, text: &str) -> Message {
    Message {
        ts: ts.to_owned(),
        author: author.to_owned(),
        text: text.to_owned(),
    }
}

/// A thread read whole: every message it held was kept.
fn thread(messages: Vec<Message>) -> Replies {
    let seen = messages.len();
    Replies { messages, seen }
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
    assert!(!composed.thread.truncated);
}

/// A thread holding only the mention tells nothing and adds no heading.
#[test]
fn a_thread_of_only_the_mention_adds_nothing() {
    let read = Ok(thread(vec![said(MENTION_TS, "U02", "<@UBOT> status?")]));
    let composed = compose("status?", MENTION_TS, &read);
    assert_eq!(composed.message, "status?");
    assert!(composed.thread.fetched);
    assert_eq!(composed.thread.count, 0);
    assert!(!composed.thread.truncated);
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

/// One long message is cut at its own cap, on a character boundary, and the
/// thread says a cap cut it.
#[test]
fn one_message_is_cut_at_its_cap() {
    let long = "é".repeat(MESSAGE_CAP + 500);
    let composed = compose(
        "why?",
        MENTION_TS,
        &Ok(thread(vec![said("1", "U01", &long)])),
    );
    let told = composed
        .message
        .split_once("- U01: ")
        .map_or("", |(_, told)| told);
    assert_eq!(told.chars().count(), MESSAGE_CAP);
    assert!(composed.thread.truncated);
}

/// A thread over budget keeps the parent and the newest replies that fit,
/// in thread order, and stays within the thread cap.
#[test]
fn the_budget_keeps_the_parent_and_the_newest_replies() {
    let full = "x".repeat(MESSAGE_CAP);
    let messages: Vec<Message> = (0..20)
        .map(|index| said(&index.to_string(), &format!("U{index:02}"), &full))
        .collect();
    let composed = compose("why?", MENTION_TS, &Ok(thread(messages)));

    let thread_text = composed
        .message
        .split_once(THREAD_HEADING)
        .map_or("", |(_, told)| told);
    assert!(thread_text.chars().count() <= THREAD_CAP + composed.thread.count);
    assert!(composed.thread.truncated);
    let authors: Vec<&str> = thread_text
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| line.split_once(':').map(|(author, _)| author))
        .collect();
    assert_eq!(authors.first(), Some(&"U00"), "the parent is always told");
    assert_eq!(authors.last(), Some(&"U19"), "the newest reply is told");
    assert!(
        authors.windows(2).all(|pair| pair.first() < pair.last()),
        "in thread order: {authors:?}"
    );
    assert_eq!(composed.thread.count, authors.len());
    assert!(authors.len() < 20, "the oldest replies were left out");
}

/// A window that dropped replies before composing marks the thread cut even
/// when everything it kept fits.
#[test]
fn replies_the_window_dropped_count_as_truncated() {
    let read = Ok(Replies {
        messages: vec![said("1", "U01", "parent"), said("30", "U02", "latest")],
        seen: 30,
    });
    let composed = compose("why?", MENTION_TS, &read);
    assert_eq!(composed.thread.count, 2);
    assert!(composed.thread.truncated);
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
