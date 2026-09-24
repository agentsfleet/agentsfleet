//! Parsing and filtering a verified mention, with no datastore.

use super::{Asked, Parsed, REASON_BOT_MESSAGE, REASON_UNREADABLE, parse, without_bot_mention};
use crate::handler::webhook::REASON_UNSUPPORTED_EVENT;

/// One `event_callback` carrying an `app_mention` shaped by `event`.
fn envelope(event: &str) -> serde_json::Value {
    serde_json::from_str(&format!(
        r#"{{"type":"event_callback","team_id":"T024BE7LD","event_id":"Ev01","event":{event}}}"#
    ))
    .unwrap_or(serde_json::Value::Null)
}

/// A person's mention in a thread, as Slack sends it.
const IN_A_THREAD: &str = r#"{"type":"app_mention","user":"U01","text":"<@UBOT> why did it fail?","ts":"1700000000.000200","thread_ts":"1700000000.000100","channel":"C0123456789"}"#;

/// A person's mention that starts its own thread.
const TOP_LEVEL: &str = r#"{"type":"app_mention","user":"U01","text":"<@UBOT> status?","ts":"1700000000.000300","channel":"C0123456789"}"#;

/// A person's mention parses with the thread's root as the reply thread; one
/// that starts a thread replies under itself.
#[test]
fn a_persons_mention_parses_with_its_thread() {
    let asked = |text: &str, ts: &str, thread_ts: &str| {
        Parsed::Asked(Asked {
            team_id: "T024BE7LD".to_owned(),
            event_id: "Ev01".to_owned(),
            user: "U01".to_owned(),
            text: text.to_owned(),
            channel: "C0123456789".parse().unwrap_or_else(|_| unreachable!()),
            ts: ts.to_owned(),
            thread_ts: thread_ts.to_owned(),
        })
    };
    assert_eq!(
        parse(envelope(IN_A_THREAD)),
        asked(
            "<@UBOT> why did it fail?",
            "1700000000.000200",
            "1700000000.000100"
        ),
        "a mention in a thread replies to the thread's root"
    );
    assert_eq!(
        parse(envelope(TOP_LEVEL)),
        asked("<@UBOT> status?", "1700000000.000300", "1700000000.000300"),
        "a top-level mention replies under itself"
    );
}

/// Dimension 1.3 — bot, edited, user-less and field-missing mentions are each
/// dropped with their reason; a direct message and another event are not
/// mentions this route answers.
#[test]
fn non_human_mentions_are_dropped() {
    for (case, event, reason) in [
        (
            "a bot's post",
            r#"{"type":"app_mention","bot_id":"B01","text":"<@UBOT> hi","ts":"1","channel":"C0123456789"}"#,
            REASON_BOT_MESSAGE,
        ),
        (
            "an edit",
            r#"{"type":"app_mention","user":"U01","subtype":"message_changed","text":"<@UBOT> hi","ts":"1","channel":"C0123456789"}"#,
            REASON_BOT_MESSAGE,
        ),
        (
            "no user",
            r#"{"type":"app_mention","text":"<@UBOT> hi","ts":"1","channel":"C0123456789"}"#,
            REASON_UNREADABLE,
        ),
        (
            "no ts",
            r#"{"type":"app_mention","user":"U01","text":"<@UBOT> hi","channel":"C0123456789"}"#,
            REASON_UNREADABLE,
        ),
        (
            "a direct message",
            r#"{"type":"app_mention","user":"U01","text":"<@UBOT> hi","ts":"1","channel":"D0123456789"}"#,
            REASON_UNSUPPORTED_EVENT,
        ),
        (
            "another event",
            r#"{"type":"reaction_added","user":"U01"}"#,
            REASON_UNSUPPORTED_EVENT,
        ),
    ] {
        assert_eq!(parse(envelope(event)), Parsed::Dropped(reason), "{case}");
    }

    let missing_event_id = serde_json::from_str(&format!(
        r#"{{"type":"event_callback","team_id":"T024BE7LD","event":{IN_A_THREAD}}}"#
    ))
    .unwrap_or(serde_json::Value::Null);
    assert_eq!(
        parse(missing_event_id),
        Parsed::Dropped(REASON_UNREADABLE),
        "a body missing Slack's event id cannot be deduplicated, so it is unreadable"
    );
    let empty_team = serde_json::from_str(&format!(
        r#"{{"type":"event_callback","team_id":"","event_id":"Ev01","event":{IN_A_THREAD}}}"#
    ))
    .unwrap_or(serde_json::Value::Null);
    assert_eq!(parse(empty_team), Parsed::Dropped(REASON_UNREADABLE));
    assert_eq!(
        parse(serde_json::json!({"type": "app_rate_limited"})),
        Parsed::Dropped(REASON_UNSUPPORTED_EVENT),
        "an envelope that is not an event callback is not a mention"
    );
}

/// The bot's own leading mention is removed and the rest kept; a message not
/// starting with a mention, or with an unclosed one, is kept whole.
#[test]
fn the_leading_bot_mention_is_removed() {
    for (text, asked) in [
        ("<@UBOT> why did it fail?", "why did it fail?"),
        ("  <@UBOT>   responder: status", "responder: status"),
        ("<@UBOT>", ""),
        ("why did it fail?", "why did it fail?"),
        ("<@UBOT why", "<@UBOT why"),
    ] {
        assert_eq!(without_bot_mention(text), asked, "`{text}`");
    }
}
