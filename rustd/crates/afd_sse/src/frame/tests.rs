//! What each frame shape carries, and what the multiplex refuses to emit.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::borrow::Cow;

use super::{DEFAULT_KIND, Frame, KIND_ANCHOR, KIND_CATCHING_UP, KIND_HELLO, KIND_KEY, kind_of};
use crate::error::Error;

/// A fleet identifier, as a channel name hands one back.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// One publisher's payload, in the shape the runner writes.
const CHUNK: &str = r#"{"kind":"chunk","text":"hi"}"#;

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
    let frame = Frame::activity(7, CHUNK.to_owned());
    assert_eq!(frame.seq, 7);
    assert_eq!(frame.kind, "chunk");
    assert_eq!(frame.data, CHUNK);
}

/// A payload naming no kind still arrives, under the default name.
#[test]
fn should_forward_a_payload_that_names_no_kind() {
    let frame = Frame::activity(1, "{}".to_owned());
    assert_eq!(frame.kind, Cow::Borrowed(DEFAULT_KIND));
    assert_eq!(frame.data, "{}");
}

/// The tag is spliced ahead of the publisher's fields, which stay byte for byte.
#[test]
fn should_splice_the_fleet_ahead_of_the_publishers_fields() {
    let frame = Frame::tagged(3, FLEET, CHUNK).expect("an object takes a tag");
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
    let frame = Frame::tagged(0, FLEET, "{}").expect("an empty object is still an object");
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
            Frame::tagged(0, FLEET, payload),
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
    let frame = Frame::tagged(0, r#"a","evil":"1"#, "{}").expect("an object takes a tag");
    let parsed: serde_json::Value =
        serde_json::from_str(&frame.data).expect("the frame is valid JSON");
    assert_eq!(parsed.get("evil"), None, "the id cannot open a second key");
    assert_eq!(
        parsed.get("fleet_id").and_then(serde_json::Value::as_str),
        Some(r#"a","evil":"1"#)
    );
}

/// The hello frame announces the set, at the synthetic sequence.
#[test]
fn should_announce_the_fleet_set_without_burning_a_sequence_number() {
    let frame = Frame::hello(&["z1".to_owned(), "z2".to_owned()]);
    assert_eq!(frame.seq, 0);
    assert_eq!(frame.kind, Cow::Borrowed(KIND_HELLO));
    assert_eq!(
        frame.data, r#"{"kind":"hello","fleet_ids":["z1","z2"]}"#,
        "`preserve_order` keeps insertion order, and the client reads by name"
    );
}

/// A workspace carrying no readable fleet still announces itself.
///
/// The client needs the frame to know the connection is live and the wall is
/// empty — silence is indistinguishable from a stream that never opened.
#[test]
fn should_announce_an_empty_fleet_set() {
    let frame = Frame::hello(&[]);
    assert_eq!(frame.data, r#"{"kind":"hello","fleet_ids":[]}"#);
}

/// The catching-up frame carries the count, at the synthetic sequence.
#[test]
fn should_report_dropped_frames_without_burning_a_sequence_number() {
    let frame = Frame::catching_up(3);
    assert_eq!(frame.seq, 0);
    assert_eq!(frame.kind, Cow::Borrowed(KIND_CATCHING_UP));
    assert_eq!(frame.data, r#"{"kind":"catching_up","dropped":3}"#);
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
            Frame::activity(0, payload.clone()).kind,
            DEFAULT_KIND,
            "the frame still arrives, under the default name"
        );
    }
}
