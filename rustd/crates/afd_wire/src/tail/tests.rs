//! What each frame carries, and the shape the SSE layer reads it through.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::borrow::Cow;

use serde_json::{Value, json};

use super::{FleetCounters, TailFrame, TailRow};
use crate::event::EventSummary;

/// The two columns the channel names and the completion leaves off.
const SCOPE_KEYS: [&str; 2] = ["fleet_id", "workspace_id"];

/// The anchor the SSE layer reads the frame's name from.
const KIND_ANCHOR: &str = "{\"kind\":\"";

fn row() -> EventSummary<'static> {
    EventSummary {
        fleet_id: Cow::Borrowed("fleet-1"),
        event_id: Cow::Borrowed("1725000000000-0"),
        workspace_id: Cow::Borrowed("ws-1"),
        actor: Cow::Borrowed("steer:user_1"),
        event_type: Cow::Borrowed("chat"),
        status: Cow::Borrowed("processed"),
        tokens: Some(1200),
        wall_ms: Some(12_000),
        failure_label: None,
        failure_detail: None,
        checkpoint_id: None,
        resumes_event_id: None,
        created_at: 1_725_000_000_000,
        updated_at: 1_725_000_012_000,
        cost_nanos: Some(40_000_000),
    }
}

/// Where the fleet's counters stand, as a frame carries them.
const COUNTERS: FleetCounters = FleetCounters {
    events_processed: 7,
    budget_used_nanos: 40_000_000,
};

fn rendered(frame: &TailFrame<'_>) -> (String, Value) {
    let text = serde_json::to_string(frame).expect("every tail frame serializes");
    let value = serde_json::from_str(&text).expect("and is JSON");
    (text, value)
}

/// The name is the leading field on every variant, which is the property
/// `afd_sse` dispatches on and the one a field reorder would silently lose.
#[test]
fn should_lead_every_frame_with_its_kind() {
    let frames = [
        TailFrame::EventReceived {
            event_id: Cow::Borrowed("e"),
            actor: Cow::Borrowed("cron"),
            event_type: Cow::Borrowed("cron"),
            created_at: 1,
            counters: Some(COUNTERS),
        },
        TailFrame::EventComplete {
            event: Box::new(TailRow::from(row())),
            fleet_status: Cow::Borrowed("active"),
            pending_approvals: 0,
            counters: Some(COUNTERS),
        },
        TailFrame::GateOpened {
            gate_id: Cow::Borrowed("g"),
            event_id: Cow::Borrowed("e"),
            pending_approvals: 1,
            counters: Some(COUNTERS),
        },
        TailFrame::GateResolved {
            gate_id: Cow::Borrowed("g"),
            event_id: Some(Cow::Borrowed("e")),
            status: Cow::Borrowed("approved"),
            resolved_by: Cow::Borrowed("human:x"),
            pending_approvals: 0,
            counters: Some(COUNTERS),
        },
    ];
    let expected = [
        "event_received",
        "event_complete",
        "gate_opened",
        "gate_resolved",
    ];
    for (frame, kind) in frames.iter().zip(expected) {
        let (text, value) = rendered(frame);
        assert!(
            text.starts_with(&format!("{KIND_ANCHOR}{kind}\"")),
            "{kind} must lead its payload: {text}"
        );
        assert_eq!(value["kind"], json!(kind));
    }
}

/// The completion is the events-list row plus the two fleet facts, and
/// nothing the row carries is renamed or dropped on the way but the two
/// scope columns — the client folds it with the same decoder it uses for
/// a backfilled page, and the workspace multiplex adds the scope back as
/// its one leading `fleet_id`.
#[test]
fn should_carry_the_whole_terminal_row_on_a_completion() {
    let (text, value) = rendered(&TailFrame::EventComplete {
        event: Box::new(TailRow::from(row())),
        fleet_status: Cow::Borrowed("paused"),
        pending_approvals: 2,
        counters: Some(COUNTERS),
    });
    let row_value = serde_json::to_value(row()).expect("the row serializes");
    for (key, expected) in row_value.as_object().expect("a row is an object") {
        if SCOPE_KEYS.contains(&key.as_str()) {
            assert!(
                value.get(key).is_none(),
                "{key} is the channel's to name, not the frame's: {text}"
            );
            continue;
        }
        assert_eq!(&value[key], expected, "{key} rides the frame unchanged");
    }
    assert_eq!(value["fleet_status"], json!("paused"));
    assert_eq!(value["pending_approvals"], json!(2));
    assert_eq!(value["cost_nanos"], json!(40_000_000));
}

/// A gate that held no run says so with `null`, the crate's spelling for
/// an absent optional — never an empty string a consumer keying on the
/// identifier would take for a real one.
#[test]
fn should_spell_a_runless_gates_event_as_null() {
    let (text, value) = rendered(&TailFrame::GateResolved {
        gate_id: Cow::Borrowed("g"),
        event_id: None,
        status: Cow::Borrowed("timed_out"),
        resolved_by: Cow::Borrowed("sweeper"),
        pending_approvals: 0,
        counters: None,
    });
    assert_eq!(value["event_id"], Value::Null, "{text}");
}

/// Every frame carries the counters, not only the completion.
///
/// This is the defect the snapshot exists to close. `events_processed` moves
/// `AFTER INSERT ON core.fleet_events` — at receive — but a gate park returns
/// before the completion is published, so a client hearing counters only on
/// `event_complete` reads short by exactly the events awaiting a human. A
/// frame-by-frame assertion is what keeps a later variant from being added
/// without them.
#[test]
fn should_carry_the_counters_on_every_frame_kind() {
    let frames = [
        TailFrame::EventReceived {
            event_id: Cow::Borrowed("e"),
            actor: Cow::Borrowed("cron"),
            event_type: Cow::Borrowed("cron"),
            created_at: 1,
            counters: Some(COUNTERS),
        },
        TailFrame::EventComplete {
            event: Box::new(TailRow::from(row())),
            fleet_status: Cow::Borrowed("active"),
            pending_approvals: 0,
            counters: Some(COUNTERS),
        },
        TailFrame::GateOpened {
            gate_id: Cow::Borrowed("g"),
            event_id: Cow::Borrowed("e"),
            pending_approvals: 1,
            counters: Some(COUNTERS),
        },
        TailFrame::GateResolved {
            gate_id: Cow::Borrowed("g"),
            event_id: Some(Cow::Borrowed("e")),
            status: Cow::Borrowed("approved"),
            resolved_by: Cow::Borrowed("human:x"),
            pending_approvals: 0,
            counters: Some(COUNTERS),
        },
    ];
    for frame in &frames {
        let (text, value) = rendered(frame);
        let kind = value["kind"].as_str().expect("a frame names its kind");
        assert_eq!(
            value["events_processed"],
            json!(7),
            "{kind} dropped its event count: {text}"
        );
        assert_eq!(
            value["budget_used_nanos"],
            json!(40_000_000),
            "{kind} dropped its spend: {text}"
        );
    }
}

/// The snapshot rides FLAT, beside `pending_approvals` rather than nested
/// under a key of its own.
///
/// The client reads these off the same block it already reads `fleet_status`
/// and `pending_approvals` from, and a nested object would make the wall's
/// decoder reach one level deeper for two of the four figures it renders.
#[test]
fn should_write_the_counters_flat_beside_the_approvals_count() {
    let (text, value) = rendered(&TailFrame::GateOpened {
        gate_id: Cow::Borrowed("g"),
        event_id: Cow::Borrowed("e"),
        pending_approvals: 1,
        counters: Some(COUNTERS),
    });
    let object = value.as_object().expect("a frame is an object");
    assert!(
        object.contains_key("events_processed") && object.contains_key("budget_used_nanos"),
        "the counters must be the frame's own keys: {text}"
    );
    assert!(
        object.get("counters").is_none(),
        "the snapshot is flat, not nested under its field name: {text}"
    );
}

/// A counter the daemon could not read is ABSENT, never zero.
///
/// Publishing is best-effort, so a read that did not answer has to leave the
/// figures off entirely — a zero would reach a tile as a fleet that has done
/// nothing, which is a worse lie than the stale number the client already
/// holds. Absent keys are what let the client leave its own values standing.
#[test]
fn should_omit_the_counters_entirely_when_none_were_read() {
    let (text, value) = rendered(&TailFrame::EventReceived {
        event_id: Cow::Borrowed("e"),
        actor: Cow::Borrowed("cron"),
        event_type: Cow::Borrowed("cron"),
        created_at: 1,
        counters: None,
    });
    let object = value.as_object().expect("a frame is an object");
    assert!(
        object.get("events_processed").is_none(),
        "an unread counter must be absent, not zero: {text}"
    );
    assert!(
        object.get("budget_used_nanos").is_none(),
        "an unread counter must be absent, not zero: {text}"
    );
    // The frame still arrives and still names itself: an absent snapshot
    // costs the tail its figures, never the marker.
    assert_eq!(value["kind"], json!("event_received"));
}
