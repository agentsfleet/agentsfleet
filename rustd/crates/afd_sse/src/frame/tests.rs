//! What each frame shape carries, and what the multiplex refuses to emit.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use afd_dragonfly::Message;
use afd_wire::tail::FleetCounters;

use super::{
    DEFAULT_KIND, Frame, KIND_ACCESS_REVOKED, KIND_ANCHOR, KIND_CATCHING_UP, KIND_HELLO, KIND_KEY,
    kind_of,
};
use crate::error::Error;

/// A fleet identifier, as a channel name hands one back.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// One publisher's payload, in the shape the runner writes.
const CHUNK: &str = r#"{"kind":"chunk","text":"hi"}"#;

/// `payload` as the hub hands it to every reader of its channel.
fn published(payload: &str) -> Arc<Message> {
    Arc::new(Message {
        channel: format!("fleet:{FLEET}:activity"),
        payload: payload.to_owned(),
    })
}

/// The leading `kind` field names the frame.
#[test]
fn should_name_the_frame_after_the_payloads_leading_kind() {
    assert_eq!(kind_of(CHUNK), Some("chunk"));
    assert_eq!(
        kind_of(r#"{"kind":"event_received","event_id":"x"}"#),
        Some("event_received")
    );
}

/// A `kind` that is not the leading field is not the frame's name.
///
/// The anchor is what stops an embedded `"kind":"` inside somebody's prose from
/// deciding the client's dispatch — a chunk of text quoting this very shape
/// must still arrive as a chunk.
#[test]
fn should_read_no_kind_from_anywhere_but_the_leading_field() {
    for payload in [
        r#"{"event_id":"x","kind":"chunk"}"#,
        r#"{"text":"\"kind\":\"fake\""}"#,
        r#"{"kind":""}"#,
        "{}",
        "",
        r#"{"k"#,
        "not json",
    ] {
        assert_eq!(kind_of(payload), None, "{payload} names no kind");
    }
}

/// A payload the publisher wrote crosses the wire unrewritten.
#[test]
fn should_forward_a_payload_without_rewriting_it() {
    let frame = Frame::activity(7, published(CHUNK));
    assert_eq!(frame.seq, 7);
    assert_eq!(frame.kind, "chunk");
    assert_eq!(frame.data, CHUNK);
}

/// A payload naming no kind still arrives, under the default name.
#[test]
fn should_forward_a_payload_that_names_no_kind() {
    let frame = Frame::activity(1, published("{}"));
    assert_eq!(frame.kind, Cow::Borrowed(DEFAULT_KIND));
    assert_eq!(frame.data, "{}");
}

/// The tag is spliced ahead of the publisher's fields, which stay byte for byte.
#[test]
fn should_splice_the_fleet_ahead_of_the_publishers_fields() {
    let frame = Frame::tagged(3, FLEET, published(CHUNK)).expect("an object takes a tag");
    assert_eq!(frame.seq, 3);
    assert_eq!(
        frame.kind, "chunk",
        "the kind is read before the tag displaces it"
    );
    assert_eq!(
        frame.data,
        format!(r#"{{"fleet_id":"{FLEET}","kind":"chunk","text":"hi"}}"#)
    );
}

/// An empty object gains the tag and stays valid JSON.
#[test]
fn should_tag_an_empty_object_without_a_dangling_separator() {
    let frame =
        Frame::tagged(0, FLEET, published("{}")).expect("an empty object is still an object");
    assert_eq!(frame.data, format!(r#"{{"fleet_id":"{FLEET}"}}"#));
    assert_eq!(frame.kind, Cow::Borrowed(DEFAULT_KIND));
}

/// A payload that is not an object is refused, and no frame is built.
///
/// Publisher shape drift must not produce a half-spliced frame: there is
/// nothing to splice into, and a malformed `data` line would break the client's
/// parser for every later frame on the connection.
#[test]
fn should_refuse_to_tag_a_payload_that_is_not_an_object() {
    for payload in ["not json", "[", "[]", "", "{", "}", r#""a string""#, "7"] {
        assert_eq!(
            Frame::tagged(0, FLEET, published(payload)),
            Err(Error::Untaggable),
            "{payload} is not an object"
        );
    }
}

/// A fleet id reaches the wire through JSON's own escaping.
///
/// Every id this daemon subscribes with is a UUID, so the quote here can only
/// arrive through a bug — and a bug that produced one must not be able to close
/// the string and write its own fields into the frame.
#[test]
fn should_escape_the_tag_rather_than_trust_the_identifier() {
    let frame =
        Frame::tagged(0, r#"a","evil":"1"#, published("{}")).expect("an object takes a tag");
    let parsed: serde_json::Value =
        serde_json::from_str(&frame.data.text()).expect("the frame is valid JSON");
    assert_eq!(parsed.get("evil"), None, "the id cannot open a second key");
    assert_eq!(
        parsed.get("fleet_id").and_then(serde_json::Value::as_str),
        Some(r#"a","evil":"1"#)
    );
}

/// The hello frame announces the set, at the synthetic sequence.
#[test]
fn should_announce_the_fleet_set_without_burning_a_sequence_number() {
    let frame = Frame::hello(&["z1".to_owned(), "z2".to_owned()], &BTreeMap::new());
    assert_eq!(frame.seq, 0);
    assert_eq!(frame.kind, Cow::Borrowed(KIND_HELLO));
    assert_eq!(
        frame.data, r#"{"kind":"hello","fleet_ids":["z1","z2"],"counters":{}}"#,
        "`preserve_order` keeps insertion order, and the client reads by name"
    );
}

/// A subscriber that arrives after the fleet has run is told where it stands
/// in the `hello`, before any event frame — the figures the page rendered
/// are the figures the wall shows, with no frame owed in between.
#[test]
fn hello_carries_the_counters_a_late_subscriber_missed() {
    let counters = BTreeMap::from([(
        "z1".to_owned(),
        FleetCounters {
            events_processed: 3,
            budget_used_nanos: 21,
        },
    )]);
    let frame = Frame::hello(&["z1".to_owned(), "z2".to_owned()], &counters);
    let parsed: serde_json::Value =
        serde_json::from_str(&frame.data.text()).expect("hello is JSON");
    assert_eq!(
        parsed.pointer("/counters/z1/events_processed"),
        Some(&serde_json::json!(3)),
        "the three events that ran before the subscribe are reported before any frame"
    );
    assert_eq!(
        parsed.pointer("/counters/z1/budget_used_nanos"),
        Some(&serde_json::json!(21))
    );
    assert!(
        parsed.pointer("/counters/z2").is_none(),
        "a fleet the read did not answer for is omitted, never zeroed"
    );
}

/// A workspace carrying no readable fleet still announces itself.
///
/// The client needs the frame to know the connection is live and the wall is
/// empty — silence is indistinguishable from a stream that never opened.
#[test]
fn should_announce_an_empty_fleet_set() {
    let frame = Frame::hello(&[], &BTreeMap::new());
    assert_eq!(
        frame.data,
        r#"{"kind":"hello","fleet_ids":[],"counters":{}}"#
    );
}

/// The catching-up frame carries the count, at the synthetic sequence.
#[test]
fn should_report_dropped_frames_without_burning_a_sequence_number() {
    let frame = Frame::catching_up(3);
    assert_eq!(frame.seq, 0);
    assert_eq!(frame.kind, Cow::Borrowed(KIND_CATCHING_UP));
    assert_eq!(frame.data, r#"{"kind":"catching_up","dropped":3}"#);
}

/// The frame a revoked stream ends on is a control frame, named for what it
/// says, carrying the code the caller's next request would be refused with.
#[test]
fn should_end_a_revoked_stream_on_a_frame_naming_its_refusal() {
    let frame = Frame::access_revoked("UZ-AUTH-001");
    assert_eq!(frame.seq, 0, "a control frame never advances the sequence");
    assert_eq!(frame.kind, Cow::Borrowed(KIND_ACCESS_REVOKED));
    assert_eq!(
        frame.data,
        r#"{"kind":"access_revoked","error_code":"UZ-AUTH-001"}"#
    );
}

/// The anchor the activity frames are READ through is built from the same key
/// the control frames are WRITTEN with.
///
/// `KIND_ANCHOR` has to be a literal — a `const` cannot be formatted at compile
/// time here — so this is what stops the two from drifting apart. Were the key
/// ever renamed and the anchor left behind, every activity frame would lose its
/// `event:` name and silently arrive as `message`: a regression no assertion on
/// either constant alone could see.
#[test]
fn should_anchor_on_the_same_key_the_control_frames_write() {
    assert_eq!(KIND_ANCHOR, format!("{{\"{KIND_KEY}\":\""));
}

/// A kind carrying a control character is refused, and the frame still arrives
/// under the default name.
///
/// The `event:` line is written by `axum`, which PANICS on a newline or a
/// carriage return in it. Nothing upstream rules one out — the kind is SLICED
/// out of the publisher's bytes rather than parsed — so the refusal is what
/// stands between a drifted payload and a dead connection. Asserted over the
/// whole control range rather than over `\n` alone: the panic names two
/// characters, and a check that admitted the other thirty would be one somebody
/// has to rediscover.
#[test]
fn should_refuse_a_kind_that_would_break_the_event_line() {
    for raw in ["a\nb", "a\rb", "\n", "\r\n", "a\u{0}b", "a\tb"] {
        let payload = format!(r#"{{"kind":"{raw}","text":"hi"}}"#);
        assert_eq!(
            kind_of(&payload),
            None,
            "{raw:?} must not reach the event: line"
        );
        assert_eq!(
            Frame::activity(0, published(&payload)).kind,
            DEFAULT_KIND,
            "the frame still arrives, under the default name"
        );
    }
}

/// Dimension 5.4, the frame's half: an activity frame and a wall frame built
/// for three viewers all point at the one payload the hub dispatched, and
/// only the tag's head is the wall frame's own.
#[test]
fn test_frames_share_the_published_payload() {
    let message = published(CHUNK);
    let viewers: Vec<Frame> = (0..3)
        .map(|seq| Frame::activity(seq, Arc::clone(&message)))
        .collect();
    for frame in &viewers {
        let (shared, skip) = frame.data.shared().expect("an activity frame shares");
        assert!(Arc::ptr_eq(shared, &message) && skip == 0);
        assert_eq!(frame.data.tail().as_ptr(), message.payload.as_ptr());
    }

    let wall = Frame::tagged(0, FLEET, Arc::clone(&message)).expect("an object takes a tag");
    assert_eq!(wall.data.head(), format!(r#"{{"fleet_id":"{FLEET}","#));
    assert_eq!(
        wall.data.tail().as_ptr(),
        message.payload[1..].as_ptr(),
        "the publisher's fields are the payload's own bytes, after its brace"
    );
    assert_eq!(
        Arc::strong_count(&message),
        5,
        "the test, three viewers, the wall"
    );
}

/// Text, display and equality all read the line a client would see.
#[test]
fn a_shared_line_reads_as_its_text() {
    let wall = Frame::tagged(0, FLEET, published(CHUNK)).expect("an object takes a tag");
    let expected = format!(r#"{{"fleet_id":"{FLEET}","kind":"chunk","text":"hi"}}"#);
    assert_eq!(wall.data.text(), expected);
    assert_eq!(wall.data.to_string(), expected);
    assert_eq!(format!("{:?}", wall.data), format!("{expected:?}"));
    assert_ne!(wall.data, format!(r#"{{"fleet_id":"{FLEET}""#));
    assert!(!wall.data.is_empty());
    assert!(Frame::activity(0, published("")).data.is_empty());
    assert!(matches!(
        Frame::activity(0, published(CHUNK)).data.text(),
        Cow::Borrowed(_)
    ));
}

/// Two frames are equal when a client reads the same text from them, however
/// each holds it: separately published copies of one payload are equal, and a
/// different payload is not.
#[test]
fn frames_are_equal_when_a_client_reads_the_same_text() {
    let one = Frame::activity(0, published(CHUNK));
    let copy = Frame::activity(0, published(CHUNK));
    assert_eq!(one, copy);
    assert_ne!(
        one,
        Frame::activity(0, published(r#"{"kind":"chunk","text":"no"}"#))
    );
    assert_eq!(
        Frame::tagged(0, FLEET, published("{}"))
            .expect("an object takes a tag")
            .data,
        Frame::tagged(0, FLEET, published("{}"))
            .expect("an object takes a tag")
            .data,
    );
}
