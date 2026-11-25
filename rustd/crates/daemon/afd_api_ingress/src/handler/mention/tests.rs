//! Parsing and filtering a verified mention, with no datastore.

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use afd_connector::grant::BotIdentity;

use super::{
    Asked, Parsed, REASON_BOT_MESSAGE, REASON_BOT_USER_UNKNOWN, REASON_UNREADABLE, parse,
    spoken_by_us, unserialisable, without_bot_mention,
};
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

/// An app's post carries Slack's `bot_id` and a real user: an alert app, or a
/// person's own token posting through an app. Either is a mention like any
/// other; only this daemon's own bot user is dropped, after the grant is read.
#[test]
fn an_apps_mention_with_a_user_wakes() {
    for (case, user) in [("an alert app", "U0PAGERDUTY"), ("a person's token", "U01")] {
        let event = format!(
            r#"{{"type":"app_mention","user":"{user}","bot_id":"B0ALERTS","text":"<@UBOT> disk full","ts":"1700000000.000400","channel":"C0123456789"}}"#
        );
        assert_eq!(
            parse(envelope(&event)),
            Parsed::Asked(Asked {
                team_id: "T024BE7LD".to_owned(),
                event_id: "Ev01".to_owned(),
                user: user.to_owned(),
                text: "<@UBOT> disk full".to_owned(),
                channel: "C0123456789".parse().unwrap_or_else(|_| unreachable!()),
                ts: "1700000000.000400".to_owned(),
                thread_ts: "1700000000.000400".to_owned(),
            }),
            "{case}"
        );
    }
}

/// Classic bot, edited, user-less and field-missing mentions are each
/// dropped with their reason; a direct message and another event are not
/// mentions this route answers.
#[test]
fn non_human_mentions_are_dropped() {
    for (case, event, reason) in [
        (
            "a classic bot or webhook post",
            r#"{"type":"app_mention","subtype":"bot_message","bot_id":"B01","text":"<@UBOT> hi","ts":"1","channel":"C0123456789"}"#,
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

/// The workspace the guard logs a fail-closed drop under.
fn workspace() -> Uuid7 {
    Uuid7::parse("01920000-0000-7000-8000-0000000000a1").unwrap_or_else(|_| unreachable!())
}

/// The bot a grant speaks as, known by `user_id` or not.
fn bot(user_id: Option<&str>) -> BotIdentity {
    BotIdentity {
        token: SecretString::new("xoxb-fixture".to_owned()),
        user_id: user_id.map(str::to_owned),
    }
}

/// The bot's own reply is dropped as `bot_message`; a person or another app
/// is let through.
#[test]
fn only_the_bots_own_user_is_dropped() {
    let workspace = workspace();
    assert_eq!(
        spoken_by_us(&workspace, &bot(Some("UBOT")), "UBOT"),
        Some(REASON_BOT_MESSAGE),
        "the bot's own reply mentions itself and would loop"
    );
    for user in ["U01", "U0PAGERDUTY", "ubot"] {
        assert_eq!(
            spoken_by_us(&workspace, &bot(Some("UBOT")), user),
            None,
            "`{user}` is not the bot"
        );
    }
}

/// A grant that recorded no bot user id cannot tell the bot's own reply from
/// a person's, so every mention is dropped rather than risk a loop.
#[test]
fn a_grant_with_no_bot_user_drops_every_mention() {
    let workspace = workspace();
    for user in ["U01", "UBOT"] {
        assert_eq!(
            spoken_by_us(&workspace, &bot(None), user),
            Some(REASON_BOT_USER_UNKNOWN),
            "`{user}` under a grant naming no bot user"
        );
    }
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

/// A body this daemon could not serialise is its own fault: a 500 carrying
/// the fixed sentence, never a 4xx that would blame the sender, and never the
/// serializer's message.
#[test]
fn an_unserialisable_body_is_this_daemons_own_500() {
    assert_eq!(
        unserialisable().status(),
        http::StatusCode::INTERNAL_SERVER_ERROR
    );
}
