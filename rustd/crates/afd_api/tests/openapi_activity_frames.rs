//! The fleet stream's description names every activity frame the channel
//! publishes, under the name the channel publishes it.
//!
//! The kinds are read off frames `afd_fleet` renders through the channel's own
//! translation, so a renamed kind fails here the way it would fail a client.
//! `afd_fleet`'s exhaustive slot match is the compile-time half; the slot check
//! below fails until every variant is rendered.
#![cfg(all(feature = "test-util", feature = "openapi"))]
#![expect(
    clippy::expect_used,
    reason = "test target: an unreadable document is a precondition failure"
)]

use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_fleet::lease::activity::{ACTIVITY_VARIANTS, RenderedFrame, Target, rendered_frames};
use jiff::Timestamp;
use serde_json::Value;

/// The fleet stream's path in the document.
const FLEET_STREAM: &str = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream";

/// The field every frame's kind is written under.
const KIND: &str = "kind";

/// The field a tool frame names its call under.
const CALL_ID: &str = "call_id";

/// The lease the sample frames are published for.
fn target() -> Target {
    Target {
        fleet_id: Uuid7::encode(Timestamp::UNIX_EPOCH, [0; ENTROPY_LEN])
            .expect("a fixed identifier encodes"),
        event_id: String::new(),
        lease_created_at: jiff::Timestamp::UNIX_EPOCH,
        event_created_at: jiff::Timestamp::UNIX_EPOCH,
        timing_eligible: true,
        fence: 1,
    }
}

/// The stream route's description in the generated document.
fn description() -> String {
    serde_json::to_value(afd_api::openapi::document())
        .expect("the generated document serializes")
        .pointer(&format!(
            "/paths/{}/get/description",
            FLEET_STREAM.replace('/', "~1")
        ))
        .and_then(Value::as_str)
        .expect("the fleet stream is described")
        .to_owned()
}

/// The `kind` one side of a rendered frame carries.
fn kind_of(frame: &Value) -> &str {
    frame
        .get(KIND)
        .and_then(Value::as_str)
        .or_else(|| frame.as_object()?.keys().next().map(String::as_str))
        .expect("every frame names its kind")
}

#[test]
fn the_stream_description_names_every_activity_frame_as_published() {
    let description = description();
    let frames: Vec<RenderedFrame> = rendered_frames(&target()).expect("every sample renders");
    let mut built = [false; ACTIVITY_VARIANTS];
    for frame in &frames {
        *built
            .get_mut(frame.slot)
            .expect("a slot past ACTIVITY_VARIANTS: raise it for the new variant") = true;
    }
    assert!(
        built.iter().all(|filled| *filled),
        "every variant is rendered: {built:?}"
    );

    for frame in &frames {
        let published = kind_of(&frame.published);
        let sent = kind_of(&frame.sent);
        assert!(
            description.contains(&format!("`{published}`")),
            "the stream description never names `{published}`"
        );
        if sent != published {
            assert!(
                !description.contains(&format!("`{sent}`")),
                "the stream publishes `{published}`, never the runner's own `{sent}`"
            );
        }
        if frame.published.get(CALL_ID).is_some() {
            assert!(
                description.contains(&format!("`{CALL_ID}`")),
                "`{published}` carries `{CALL_ID}`, which the description never names"
            );
        }
    }
}
