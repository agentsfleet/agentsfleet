//! The tail lane, run whole against the rig's Dragonfly.
//!
//! Its own test binary, because it installs the counting allocator the lane
//! binary installs: without it the ladder has no allocation or heap figures,
//! and this test would be proving a report the make target never writes.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints this without
//! datastores.

mod support;

use afd_bench::RunPrefix;
use afd_bench::allocations::Counting;
use afd_bench::lane::tail;
use afd_bench::profile::Profile;

use self::support::{LANE, datastores, measurement, series, swept};

#[global_allocator]
static COUNTING: Counting = Counting;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
async fn bench_tail_reports_fanout_and_faults() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();

    let report = swept(
        &stores,
        &prefix,
        tail::run(Profile::Rig, support::provenance(), &stores, &prefix).await,
    )
    .await;
    println!("{}", tail::summary(&report));

    let rungs = tail::VIEWER_LADDER.len() * tail::PAYLOAD_LADDER.len();
    assert!(
        measurement(&report, tail::FRAMES_UNDELIVERED).abs() < f64::EPSILON,
        "every frame reached every viewer on every rung"
    );
    assert!(
        measurement(&report, tail::LAG_NOTICES).abs() < f64::EPSILON,
        "no viewer was lapped: the ladder measured delivery, not the lag path"
    );
    assert!(
        measurement(&report, tail::STREAMS_UNREACHED).abs() < f64::EPSILON,
        "every stream on the stream ladder received its frame"
    );
    for key in [
        "allocations_per_delivered_frame",
        "busy_us_per_delivered_frame",
        "receive_p95_ms",
    ] {
        assert_eq!(
            series(&report, key).len(),
            rungs,
            "{key} has one value per rung"
        );
    }
    assert_eq!(
        series(&report, "heap_bytes_per_stream").len(),
        tail::STREAM_LADDER.len(),
        "a heap figure for every stream rung"
    );
}
