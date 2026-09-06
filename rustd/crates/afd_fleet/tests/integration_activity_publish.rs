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
use afd_core::event::label;
use afd_core::id::Uuid7;
use afd_fleet::lease::Ended;
use afd_fleet::lease::admit::Refusal;
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

/// The daemon's own closing bracket is best-effort for the reason the
/// runner's frames are.
///
/// The refusal is written over live Postgres and answers the closing it
/// wrote; announcing that closing on a queue that will not take it must cost
/// the tail one frame and the verb nothing — the row stands, and the client's
/// reconnect backfill carries it. A panic or a hang here would turn a telemetry
/// outage into a lease verb that never answers its runner.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_bracket_publish_redis_down_does_not_fail_the_closing() {
    let run = held().await;
    let plane = run.fixtures.plane_with_dead_queue();
    let fleet = Uuid7::parse(&run.fleet).expect("the seeded fleet id is well formed");

    let ended = plane
        .leases
        .block(
            &fleet,
            &run.event_id,
            Refusal::labelled(label::APPROVAL_DENIED),
            run.now,
        )
        .await
        .expect("the refusal writes over live Postgres whatever the queue does");
    let closed = match ended {
        Ended::Now(closed) => closed,
        Ended::Already => unreachable!("a held run's row is still open"),
    };
    assert_eq!(closed.row.status, afd_core::event::status::GATE_BLOCKED);

    // Returns, rather than erroring or hanging: the queue is asked once and its
    // refusal is accounted in the log, not on the verb.
    plane.leases.publish_completion(&closed).await;
}

/// Dimension 1.3 — the closing counts the fleet's pending gates, and only
/// those.
///
/// The count on a completion is what the strip shows beside "approvals
/// waiting", so it is proven off zero: one pending gate and one answered gate
/// on the held fleet, and the closing says ONE. A mis-bound status, a wrong
/// correlation column or a bind in the wrong slot would all read zero and
/// pass the bracket suite, which closes a fleet with no gates at all.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_closing_counts_the_fleets_pending_gates() {
    let run = held().await;
    let fleet = Uuid7::parse(&run.fleet).expect("the seeded fleet id is well formed");
    seed_gate(&run, afd_wire::approval::status::PENDING).await;
    seed_gate(&run, afd_wire::approval::status::DENIED).await;

    let ended = run
        .fixtures
        .plane()
        .leases
        .block(
            &fleet,
            &run.event_id,
            Refusal::labelled(label::APPROVAL_DENIED),
            run.now,
        )
        .await
        .expect("the refusal writes over live Postgres");
    let closed = match ended {
        Ended::Now(closed) => closed,
        Ended::Already => unreachable!("a held run's row is still open"),
    };
    assert_eq!(
        closed.pending_approvals, 1,
        "the pending gate counts and the answered one does not"
    );
    assert_eq!(
        closed.fleet_status, "active",
        "the closing joins the fleet's own status"
    );
}

/// One gate row on the held fleet, in `status`, shaped as the park writes it.
async fn seed_gate(run: &report_seed::Held, status: &str) {
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let workspace: String =
        sqlx::query_scalar("SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid")
            .bind(&run.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the held fleet has a workspace");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_kind, proposed_action, evidence, blast_radius, timeout_at,
            resolved_by, status, detail, created_at, updated_at, event_id)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                 'repository_write', 'open a pull request', '{}'::jsonb,
                 'one repository', $5, '', $6, '', $5, NULL, $7)",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(&run.fleet)
    .bind(&workspace)
    .bind(afd_db::test_util::mint_id())
    .bind(run.now.as_millis())
    .bind(status)
    .bind(&run.event_id)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");
}

/// One chunk frame, which is the smallest thing the tail carries.
fn chunk() -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed(CHUNK_TEXT),
    })
}
