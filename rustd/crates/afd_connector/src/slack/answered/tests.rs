//! The marker an answer is posted with, and the message check that finds it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::{ANSWER_EVENT_TYPE, AnswerMarker, CLOCK_SKEW_SECONDS, Posted, since};

/// The bot user the grant recorded: the only author whose marker counts.
const BOT_USER: &str = "U0BOTAF01";

fn marker() -> AnswerMarker {
    AnswerMarker {
        fleet_id: "0195b4ba-8d3a-7a11-8abc-000000000003".to_owned(),
        event_id: "1760000000001-0".to_owned(),
    }
}

fn posted(json: &str) -> Posted {
    serde_json::from_str(json).expect("the fixture is a Slack message")
}

/// What is posted is what a read looks for: the stamp serialized into a
/// message reads back as carrying the same marker.
#[test]
fn a_posted_stamp_reads_back_as_its_own_answer() {
    let marker = marker();
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    let message = posted(&format!(
        r#"{{"ts":"1","user":"{BOT_USER}","text":"answer","metadata":{stamp}}}"#
    ));

    assert!(message.carries(&marker, BOT_USER));
    assert!(stamp.contains(ANSWER_EVENT_TYPE), "{stamp}");
}

/// Only this answer's own marker counts: another event, another fleet,
/// another app's event type, and a message with no metadata are all a thread
/// that does not yet hold the answer.
#[test]
fn only_the_same_answer_counts() {
    let marker = marker();
    let others = [
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{"event_type":"agentsfleet_answer","event_payload":{"fleet_id":"0195b4ba-8d3a-7a11-8abc-000000000003","event_id":"1760000000002-0"}}}"#,
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{"event_type":"agentsfleet_answer","event_payload":{"fleet_id":"0195b4ba-8d3a-7a11-8abc-000000000009","event_id":"1760000000001-0"}}}"#,
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{"event_type":"task_created","event_payload":{"fleet_id":"0195b4ba-8d3a-7a11-8abc-000000000003","event_id":"1760000000001-0"}}}"#,
        r#"{"ts":"1","user":"U0BOTAF01","text":"a person's reply"}"#,
    ];
    for other in others {
        assert!(!posted(other).carries(&marker, BOT_USER), "{other}");
    }
}

/// Another app's metadata, shaped however it likes, parses and is simply not
/// ours: a page carrying it must not fail the read it is on.
#[test]
fn foreign_metadata_never_fails_a_message() {
    for foreign in [
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{"event_type":"deploy","event_payload":[1,2,3]}}"#,
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{"event_type":"deploy"}}"#,
        r#"{"ts":"1","user":"U0BOTAF01","metadata":{}}"#,
    ] {
        assert!(!posted(foreign).carries(&marker(), BOT_USER), "{foreign}");
    }
}

/// Any app in a channel can post message metadata. This answer's exact
/// marker under another author, or under a bot with no user, is a thread
/// that does not hold the answer: the repeat posts rather than going silent.
#[test]
fn a_marker_another_author_posted_is_not_ours() {
    let marker = marker();
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    for forged in [
        format!(r#"{{"ts":"1","user":"U0OTHERAPP","metadata":{stamp}}}"#),
        format!(r#"{{"ts":"1","bot_id":"B0OTHER","metadata":{stamp}}}"#),
    ] {
        assert!(!posted(&forged).carries(&marker, BOT_USER), "{forged}");
    }
}

/// The read starts [`CLOCK_SKEW_SECONDS`] before the instant the event id
/// opens with, as a Slack timestamp; an id with no instant reads everything.
#[test]
fn the_check_reads_from_just_before_the_question() {
    assert_eq!(
        since("1760000000001-0").as_deref(),
        Some("1759999700.000000")
    );
    assert_eq!(CLOCK_SKEW_SECONDS, 300, "the expectation above is the skew");
    assert_eq!(
        since("120000-3").as_deref(),
        Some("0.000000"),
        "never negative"
    );
    for unreadable in ["T024BE7LD:Ev01:notice", "1760000000001", "-0", "x-0"] {
        assert_eq!(since(unreadable), None, "{unreadable}");
    }
}
