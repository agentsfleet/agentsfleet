//! Admission wakes a connected worker through a pushed offer without lease requests.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{EVENT_TYPE, scenario},
    wire,
};
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn admitted_work_reaches_an_idle_native_worker_without_a_lease_poll() {
    let mut supervisor = Supervisor::new();
    let mut run = scenario(&mut supervisor).await;
    wire::heartbeat(&run, &wire::capable_beat()).await;
    let (lease, fence) = wire::wait_for_seeded_offer(&run).await;
    wire::report(&run, &wire::report_body(&lease, &run.event_id, fence))
        .await
        .expect("settle seeded work");
    let event = run.enqueue_event(EVENT_TYPE).await;
    let (lease, fence) = wire::wait_for_offer(&run, &event).await;
    wire::report(&run, &wire::report_body(&lease, &event, fence))
        .await
        .expect("settle pushed work");
    run.native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
}
