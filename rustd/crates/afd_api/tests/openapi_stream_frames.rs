//! The fleet stream's description names every frame the daemon publishes.
//!
//! A client learns the frame vocabulary from the published description. Each
//! kind is read from the frame as serde writes it, so a renamed variant or tag
//! fails here the same way it would fail a client. An exhaustive `match` over
//! [`TailFrame`] is the compile-time half: a new variant does not compile here
//! until it has a slot, and then fails until [`one_of_each`] builds it.
#![cfg(all(feature = "test-util", feature = "openapi"))]
#![expect(
    clippy::expect_used,
    reason = "test target: an unreadable document is a precondition failure"
)]

use std::borrow::Cow;

use afd_wire::tail::{TailFrame, TailRow};
use serde_json::Value;

/// The fleet stream's path in the document.
const FLEET_STREAM: &str = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream";

/// The field `TailFrame`'s internal tag is written under.
const KIND: &str = "kind";

/// How many variants [`TailFrame`] has: one per slot [`slot`] hands out.
const VARIANTS: usize = 5;

/// Which variant `frame` is. The guard, not the source of the kinds.
const fn slot(frame: &TailFrame<'_>) -> usize {
    match frame {
        TailFrame::EventAdmitted { .. } => 0,
        TailFrame::EventReceived { .. } => 1,
        TailFrame::EventComplete { .. } => 2,
        TailFrame::GateOpened { .. } => 3,
        TailFrame::GateResolved { .. } => 4,
    }
}

/// One frame of each kind, the smallest each can be.
fn one_of_each() -> Vec<TailFrame<'static>> {
    let text = || Cow::Borrowed("x");
    vec![
        TailFrame::EventAdmitted {
            event_id: text(),
            actor: text(),
            event_type: text(),
            message: text(),
            created_at: 0,
        },
        TailFrame::EventReceived {
            event_id: text(),
            actor: text(),
            event_type: text(),
            created_at: 0,
            message: None,
            counters: None,
        },
        completion(),
        TailFrame::GateOpened {
            gate_id: text(),
            event_id: text(),
            pending_approvals: 0,
            counters: None,
        },
        TailFrame::GateResolved {
            gate_id: text(),
            event_id: None,
            status: text(),
            resolved_by: text(),
            pending_approvals: 0,
            counters: None,
        },
    ]
}

/// The smallest completion: a run's terminal row with nothing optional set.
fn completion() -> TailFrame<'static> {
    let text = || Cow::Borrowed("x");
    TailFrame::EventComplete {
        event: Box::new(TailRow {
            event_id: text(),
            actor: text(),
            event_type: text(),
            status: text(),
            tokens: None,
            wall_ms: None,
            failure_label: None,
            failure_detail: None,
            checkpoint_id: None,
            resumes_event_id: None,
            created_at: 0,
            updated_at: 0,
            cost_nanos: None,
        }),
        final_reply: None,
        fleet_status: text(),
        pending_approvals: 0,
        counters: None,
    }
}

/// The `kind` a frame is published under, as serde writes it.
fn kind_of(frame: &TailFrame<'_>) -> String {
    serde_json::to_value(frame)
        .expect("a tail frame serializes")
        .get(KIND)
        .and_then(Value::as_str)
        .expect("every tail frame is tagged with its kind")
        .to_owned()
}

#[test]
fn test_stream_description_names_every_frame() {
    let document = serde_json::to_value(afd_api::openapi::document())
        .expect("the generated document serializes");
    let description = document
        .pointer(&format!(
            "/paths/{}/get/description",
            FLEET_STREAM.replace('/', "~1")
        ))
        .and_then(Value::as_str)
        .expect("the fleet stream is described");
    let frames = one_of_each();
    let mut built = [false; VARIANTS];
    for frame in &frames {
        let filled = built
            .get_mut(slot(frame))
            .expect("a slot past VARIANTS: raise it for the new variant");
        *filled = true;
    }
    assert!(
        built.iter().all(|filled| *filled),
        "one_of_each builds every variant: {built:?}"
    );
    for kind in frames.iter().map(kind_of) {
        assert!(
            description.contains(&format!("`{kind}`")),
            "the stream description never names `{kind}`"
        );
    }
}
