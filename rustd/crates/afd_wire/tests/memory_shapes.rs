//! The memory verbs' response shapes, which no `wire-v2` fixture pins.
//!
//! `roundtrip` grades the frozen corpus. These types are not in that roster, so
//! nothing else would notice a renamed or reordered field — and a response body
//! that no fixture and no test pins is exactly the shape a handler used to
//! spell inline with `json!`.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a shape that will not serialize is an unmet precondition"
)]

use afd_wire::memory::MemoryCaptureResponse;

/// A capture reply carries the two tallies a runner acts on, and only those.
///
/// Field NAMES, not just presence: a runner reads `stored` to know its memory
/// landed and `skipped` to know some was refused for shape. Renaming either
/// silently stops a runner reacting to a refusal it can fix.
#[test]
fn test_a_capture_reply_carries_stored_and_skipped() {
    let reply = MemoryCaptureResponse {
        stored: 3,
        skipped: 1,
    };

    let json = serde_json::to_value(&reply).unwrap();

    assert_eq!(json, serde_json::json!({"stored": 3, "skipped": 1}));
}

/// The daemon's housekeeping counts stay out of the reply.
///
/// Sweep and eviction totals are computed on the same path and belong in the
/// log. `deny_unknown_fields` makes that a refusal rather than a convention, so
/// a later hand adding one has to change this test on purpose.
#[test]
fn test_a_capture_reply_refuses_a_field_it_does_not_carry() {
    let with_extra = br#"{"stored":1,"skipped":0,"evicted":7}"#;

    let refused = serde_json::from_slice::<MemoryCaptureResponse>(with_extra);

    assert!(refused.is_err(), "the reply is exactly two fields");
}

/// Both tallies survive a round trip through JSON.
#[test]
fn test_a_capture_reply_round_trips() {
    let reply = MemoryCaptureResponse {
        stored: 0,
        skipped: 12,
    };

    let bytes = serde_json::to_vec(&reply).unwrap();
    let back: MemoryCaptureResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(back, reply);
}
