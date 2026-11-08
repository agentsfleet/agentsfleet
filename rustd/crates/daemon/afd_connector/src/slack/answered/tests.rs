//! The marker an answer is posted with, and the message check that finds it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::{
    ANSWER_EVENT_TYPE, AnswerMarker, CLOCK_SKEW_SECONDS, INTERIM_EVENT_TYPE, Part, Posted, since,
};

/// The bot user the grant recorded: the only author whose marker counts.
const BOT_USER: &str = "U0BOTAF01";

fn marker() -> AnswerMarker {
    AnswerMarker {
        fleet_id: "0195b4ba-8d3a-7a11-8abc-000000000003".to_owned(),
        event_id: "1760000000001-0".to_owned(),
        part: None,
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

/// The lease a fixture interim line was said under.
const LEASE: &str = "0195b4ba-8d3a-7a11-8abc-0000000000aa";

/// Interim line `line` of `lease`, for the fixture event.
fn line(lease: &str, line: u32) -> AnswerMarker {
    AnswerMarker {
        part: Some(Part {
            lease_id: lease.to_owned(),
            line,
        }),
        ..marker()
    }
}

/// A message `BOT_USER` posted under `marker`'s stamp.
fn stamped(marker: &AnswerMarker) -> (String, Posted) {
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    let message = posted(&format!(
        r#"{{"ts":"1","user":"{BOT_USER}","text":"working on it","metadata":{stamp}}}"#
    ));
    (stamp, message)
}

/// An interim line is stamped as one, so a thread holding it does not yet
/// hold the answer, and a repeat of the answer still posts it.
#[test]
fn test_interim_marker_is_not_the_answer() {
    let interim = line(LEASE, 1);
    let (stamp, message) = stamped(&interim);

    assert!(
        !message.carries(&marker(), BOT_USER),
        "an interim line read as the answer would silence it"
    );
    assert!(
        message.carries(&interim, BOT_USER),
        "a repeat of the line finds its own"
    );
    assert!(stamp.contains(INTERIM_EVENT_TYPE), "{stamp}");
    assert!(!stamp.contains(ANSWER_EVENT_TYPE), "{stamp}");
}

/// A reclaimed lease numbers its lines from one again: its first line is not
/// the dead lease's first, so a repeat of it is never skipped as posted.
#[test]
fn test_reclaimed_lease_line_is_not_the_dead_lease_line() {
    let (_, dead) = stamped(&line(LEASE, 1));
    let reclaimed = line("0195b4ba-8d3a-7a11-8abc-0000000000bb", 1);

    assert!(!dead.carries(&reclaimed, BOT_USER));
    assert!(
        !dead.carries(&line(LEASE, 2), BOT_USER),
        "another line of one lease"
    );
}

/// The answer's marker carries no part on the wire, so every answer posted
/// before interim lines existed still reads as an answer.
#[test]
fn an_answer_marker_carries_no_part_on_the_wire() {
    let stamp = serde_json::to_string(&marker().metadata()).expect("a stamp serializes");
    assert!(!stamp.contains("part"), "{stamp}");
}
