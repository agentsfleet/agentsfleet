use super::{
    FRAMES_UNDELIVERED, HEAP_BYTES_PER_STREAM, LADDER_PAYLOAD_BYTES, LADDER_STREAMS,
    LADDER_VIEWERS, LAG_NOTICES, STREAM_RECEIVE_P95_MS, STREAMS_LIVE, STREAMS_UNREACHED, push,
    summary,
};
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

#[test]
fn the_summary_prints_one_line_per_rung_then_the_totals() {
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    for (viewers, bytes) in [(1.0, 200.0), (64.0, 4_096.0)] {
        push(&mut report, LADDER_VIEWERS, viewers);
        push(&mut report, LADDER_PAYLOAD_BYTES, bytes);
    }
    push(&mut report, LADDER_STREAMS, 64.0);
    push(&mut report, STREAMS_LIVE, 64.0);
    push(&mut report, HEAP_BYTES_PER_STREAM, 2_048.5);
    report.measurement(FRAMES_UNDELIVERED, 0.0);
    report.measurement(LAG_NOTICES, 0.0);
    report.measurement(STREAMS_UNREACHED, 0.0);

    assert_eq!(
        summary(&report),
        "ladder_viewers=1 ladder_payload_bytes=200\n\
         ladder_viewers=64 ladder_payload_bytes=4096\n\
         ladder_streams=64 streams_live=64 heap_bytes_per_stream=2048.5\n\
         frames_undelivered=0 lag_notices=0 streams_unreached=0"
    );
}

#[test]
fn a_series_the_run_never_wrote_is_left_out_of_its_rung() {
    // Without the counting allocator there is no allocation or heap figure,
    // and the line must say nothing about it rather than print a zero.
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    push(&mut report, LADDER_STREAMS, 256.0);
    push(&mut report, STREAMS_LIVE, 256.0);

    assert_eq!(summary(&report), "ladder_streams=256 streams_live=256\n");
}

#[test]
fn a_stream_rung_prints_its_publish_to_receive_p95() {
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    push(&mut report, LADDER_STREAMS, 4_096.0);
    push(&mut report, STREAMS_LIVE, 4_096.0);
    push(&mut report, STREAM_RECEIVE_P95_MS, 12.5);

    assert_eq!(
        summary(&report),
        "ladder_streams=4096 streams_live=4096 stream_receive_p95_ms=12.5\n"
    );
}
