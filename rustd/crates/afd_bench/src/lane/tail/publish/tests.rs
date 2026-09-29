#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the lane"
)]

use afd_sse::frame::kind_of;

use super::{FRAME_KIND, PROBE, PROBE_KIND, frame_of};
use crate::lane::tail::PAYLOAD_LADDER;

#[test]
fn every_rung_of_the_payload_ladder_publishes_exactly_its_size() {
    for bytes in PAYLOAD_LADDER {
        assert_eq!(frame_of(bytes).len(), bytes, "a {bytes}-byte rung");
    }
}

#[test]
fn a_measured_frame_reads_as_its_own_kind_on_the_tail() {
    // The viewer tells a measured frame from the connection's `hello` and
    // from a probe by the kind the SSE layer reads off the payload's leading
    // field. A frame that lost its kind would never be counted as delivered.
    assert_eq!(kind_of(&frame_of(PAYLOAD_LADDER[0])), Some(FRAME_KIND));
    assert_eq!(kind_of(PROBE), Some(PROBE_KIND));
}

#[test]
fn a_size_below_the_envelope_publishes_the_envelope_alone() {
    let smallest = frame_of(0);

    assert_eq!(kind_of(&smallest), Some(FRAME_KIND));
    serde_json::from_str::<serde_json::Value>(&smallest)
        .expect("the bare envelope is still a JSON object");
}
