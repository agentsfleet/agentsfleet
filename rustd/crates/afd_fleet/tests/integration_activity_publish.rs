//! Dimension 4.1's failure mode — the live tail is best-effort, and proves it.
//!
//! One claim, and it is the one a runner's whole run depends on: a queue that
//! will not take a telemetry frame must not fail the verb that forwarded it.
//! The runner counts consecutive rejections toward a self-termination ceiling,
//! so a publish failure answered as an error walks a healthy fleet's hosts to
//! shutdown one beat at a time — for a frame nobody was necessarily reading.
//!
//! # How the outage is injected, and why not by taking Redis away
//!
//! `Fixtures::plane_with_dead_queue` hands this suite a store over LIVE
//! Postgres and a Redis that will not answer. That combination is the shape a
//! partial outage actually takes, and it is the only one that reaches the
//! publish at all: a fixture with both datastores gone would refuse at the
//! first row read, never get a target, and prove nothing about the queue.
//!
//! The obvious injections — `docker compose pause redis`, killing the server,
//! dropping the port — were not used, and the reason is not tooling. The lane's
//! Redis is SHARED by every test binary `cargo test` runs in parallel, so any
//! of them fails unrelated suites at the same instant. `Redis::unreachable`
//! skips the ping `connect` performs and hands back a lazy handle pointed at a
//! closed port, so exactly one test's commands fail and nobody else notices.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::report_seed;
use std::borrow::Cow;

use afd_core::error_code;
use afd_wire::activity::{ActivityFrame, FleetResponseChunk};

use self::report_seed::held;

/// The text the forwarded frame carries. Never read back — the queue it would
/// have gone to is not answering, which is the point.
const CHUNK_TEXT: &str = "the fleet said this into a queue that is not there";

/// Dimension 4.1 (failure mode) — an unreachable queue does not fail the verb.
///
/// The runner is told the frames were RECEIVED, because they were: the lease
/// resolved, the runner owns it, and everything after that is telemetry. What
/// happened to the frames afterwards is the daemon's problem and is accounted
/// in its log, not on the runner's wire.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_activity_publish_redis_down() {
    let run = held().await;
    let plane = run.fixtures.plane_with_dead_queue();

    let forwarded = plane
        .activity(&run.runner, run.issued.lease_id.as_str(), &[chunk()])
        .await;

    assert!(
        forwarded.is_ok(),
        "a queue that would not take the frame is NOT an error: the runner \
         counts consecutive rejections toward self-termination, so a telemetry \
         outage answered as a refusal walks healthy hosts to shutdown"
    );
}

/// Dimension 4.1 (failure mode) — the ownership check still runs first.
///
/// The half a "never fails" claim could quietly break. If an unreachable queue
/// short-circuited the verb, a runner naming a lease it does not hold would get
/// the same `Ok` as the holder — and the check that stops one fleet writing
/// into another's tail would be gone precisely when nobody could see the tail
/// to notice. The authorization is a DATABASE read, and Postgres is live here,
/// so it must still refuse.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_activity_with_a_dead_queue_still_refuses_a_lease_the_runner_does_not_hold() {
    let run = held().await;
    let plane = run.fixtures.plane_with_dead_queue();

    let refusal = plane
        .activity(&run.spare, run.issued.lease_id.as_str(), &[chunk()])
        .await
        .expect_err("a runner cannot forward frames for a lease it does not hold");
    assert_eq!(
        refusal.code(),
        error_code::RUN_LEASE_NOT_FOUND,
        "the outage changes what happens to the FRAMES, never who is allowed \
         to send them"
    );
}

/// One chunk frame, which is the smallest thing the tail carries.
fn chunk() -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed(CHUNK_TEXT),
    })
}
