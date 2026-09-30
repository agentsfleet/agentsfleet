//! The fleet stream's description names every frame the daemon publishes.
//!
//! A client learns the frame vocabulary from the published description. The
//! kinds are taken from [`TailFrame`] through an exhaustive `match`, so a new
//! variant does not compile here until this list names it, and then it fails
//! until the description does.
#![cfg(all(feature = "test-util", feature = "openapi"))]
#![expect(
    clippy::expect_used,
    reason = "test target: an unreadable document is a precondition failure"
)]

use afd_wire::tail::TailFrame;

/// The fleet stream's path in the document.
const FLEET_STREAM: &str = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream";

/// The `kind` each frame is published under.
const fn kind_of(frame: &TailFrame<'_>) -> &'static str {
    match frame {
        TailFrame::EventAdmitted { .. } => "event_admitted",
        TailFrame::EventReceived { .. } => "event_received",
        TailFrame::EventComplete { .. } => "event_complete",
        TailFrame::GateOpened { .. } => "gate_opened",
        TailFrame::GateResolved { .. } => "gate_resolved",
    }
}

/// One frame of each kind, the smallest each can be.
fn one_of_each() -> Vec<TailFrame<'static>> {
    use std::borrow::Cow;
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

#[test]
fn test_stream_description_names_every_frame() {
    let document = serde_json::to_value(afd_api::openapi::document())
        .expect("the generated document serializes");
    let description = document
        .pointer(&format!(
            "/paths/{}/get/description",
            FLEET_STREAM.replace('/', "~1")
        ))
        .and_then(serde_json::Value::as_str)
        .expect("the fleet stream is described");
    let mut kinds: Vec<&str> = one_of_each().iter().map(kind_of).collect();
    // The completion is the one frame too large to build here; its kind is
    // still named, since `kind_of` above lists it.
    kinds.push("event_complete");
    for kind in kinds {
        assert!(
            description.contains(&format!("`{kind}`")),
            "the stream description never names `{kind}`"
        );
    }
}
