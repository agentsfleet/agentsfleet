//! A turn wakes a connected worker through a pushed assignment without run
//! requests.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use agentsfleetd::supervisor::Supervisor;

use crate::{
    e2e::{EVENT_TYPE, scenario},
    wire,
};

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
