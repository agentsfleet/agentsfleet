//! A turn wakes a connected worker through a pushed assignment without run
//! requests, and the replica's backstop re-read assigns a turn whose wake was lost.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use std::sync::atomic::Ordering;

use af_core::timing::ASSIGN_FALLBACK_TICK;
use afd_bus::Subject;
use agentsfleetd::supervisor::Supervisor;
use tokio::time::Instant;

use super::{RECOVERY_SLACK, authority::faults};

use crate::{
    e2e::{EVENT_TYPE, scenario},
    e2e_event::enqueue_over,
    wire,
};

/// Long enough for a returned credit's ordinary assignment to have run and found
/// nothing: on the local lane that assignment lands about 30 ms after the report.
const CREDIT_ASSIGNMENT_SETTLES: std::time::Duration = std::time::Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn admitted_work_reaches_an_idle_native_worker_without_a_run_poll() {
    let mut supervisor = Supervisor::new();
    let mut scene = scenario(&mut supervisor).await;
    wire::heartbeat(&scene, &wire::capable_beat()).await;
    let (run, fence) = wire::wait_for_seeded_assignment(&scene).await;
    wire::report(&scene, &wire::report_body(&run, &scene.turn_id, fence))
        .await
        .expect("settle seeded work");
    let event = scene.enqueue_event(EVENT_TYPE).await;
    let (run, fence) = wire::wait_for_assignment(&scene, &event).await;
    wire::report(&scene, &wire::report_body(&run, &event, fence))
        .await
        .expect("settle pushed work");
    scene
        .native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_turn_whose_wake_was_lost_is_assigned_by_the_backstop() {
    let mut supervisor = Supervisor::new();
    let mut scene = scenario(&mut supervisor).await;
    wire::heartbeat(&scene, &wire::capable_beat()).await;
    let (run, fence) = wire::wait_for_seeded_assignment(&scene).await;
    wire::report(&scene, &wire::report_body(&run, &scene.turn_id, fence))
        .await
        .expect("settle seeded work");
    let (lossy, lost) = faults::dropping(
        scene.booted.bus.clone(),
        Subject::ThreadWake(scene.fleet.clone()),
    );
    // The report hands the runner its credit back, and that credit's own assignment
    // would take a turn admitted while it runs. Waiting it out leaves the
    // backstop as the only path that can assign the next turn.
    tokio::time::sleep(CREDIT_ASSIGNMENT_SETTLES).await;
    let admitted = Instant::now();
    let event = enqueue_over(
        &scene.booted,
        lossy,
        &scene.fleet,
        &scene.workspace,
        EVENT_TYPE,
    )
    .await;
    assert_eq!(lost.load(Ordering::SeqCst), 1, "the turn's wake was lost");
    let (run, fence) = wire::wait_for_assignment(&scene, &event).await;
    let waited = admitted.elapsed();
    assert!(
        waited <= ASSIGN_FALLBACK_TICK + RECOVERY_SLACK,
        "the backstop assigns within one interval, waited {waited:?}"
    );
    wire::report(&scene, &wire::report_body(&run, &event, fence))
        .await
        .expect("settle recovered work");
    scene
        .native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    scene.cleanup().await;
}
