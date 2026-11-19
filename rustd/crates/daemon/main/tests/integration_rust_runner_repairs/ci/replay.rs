//! A hundred signed deliveries retain the identity used by the real repair loop.
use crate::definition_install::slack::Producer;
use crate::definition_repair::head_reads;
use crate::definition_run::run_event;
use crate::fake_model::{FakeModel, say};
use crate::https::Upstream;
use crate::integration_rust_runner_repairs::LINKWARDEN;
use crate::repair_remote::{Acknowledgement, Readback, Remote};
use crate::repair_walk;
use af_core::id::Uuid7;
use afd_gate::policy::repair::branch_for;
use agentsfleetd::supervisor::Supervisor;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn should_keep_one_draft_for_replayed_producer_event() {
    let mut supervisor = Supervisor::new();
    let (scene, producer) = ready_producer(&mut supervisor).await;
    let receipts = concurrent_deliveries(&producer, &scene).await;
    let event = Uuid7::parse(
        receipts[0]["event_id"]
            .as_str()
            .expect("the producer returned its event"),
    )
    .expect("the producer answers a canonical event id");
    assert_replayed_receipts(&receipts, &event, &scene.event_id);
    producer.assert_admission(&scene, &event).await;
    let remote = Remote::new(
        LINKWARDEN,
        &branch_for(event.as_str()),
        Acknowledgement::Delivered,
        Readback::Exact,
    );
    let upstream = Upstream::serve(remote.routes()).await;
    let (model, transcript) =
        FakeModel::deciding(repair_walk::script(LINKWARDEN, head_reads(LINKWARDEN)));
    let result = run_event(&scene, &event, &upstream, model).await;
    assert_eq!(
        result.answer,
        "https://github.com/agentsfleet/linkwarden/pull/9"
    );
    assert_eq!(
        (remote.state().ref_posts, remote.state().draft_posts),
        (1, 1)
    );
    assert!(
        transcript.asked()[0]
            .instructions
            .contains(&format!("repair branch: {}", branch_for(event.as_str())))
    );
    assert_eq!(
        producer.deliver(&scene).await,
        serde_json::json!({"event_id": event, "replayed": true})
    );
    producer.assert_admission(&scene, &event).await;
    assert_one_execution(&scene, &event).await;
    assert_eq!(
        (remote.state().ref_posts, remote.state().draft_posts),
        (1, 1)
    );
    assert!(
        producer.slack.reads() > 0,
        "the signed route must read its actual Slack boundary"
    );
    supervisor.shutdown().await;
    scene.cleanup().await;
}

fn assert_replayed_receipts(receipts: &[serde_json::Value], event: &Uuid7, initial: &Uuid7) {
    assert_ne!(event, initial);
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt["event_id"] == event.as_str())
    );
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| receipt["replayed"] == false)
            .count(),
        1
    );
}

async fn ready_producer(supervisor: &mut Supervisor) -> (crate::e2e::Scenario, Producer) {
    let (scene, producer) = Producer::install(supervisor).await;
    let remote = Remote::new(
        LINKWARDEN,
        &branch_for(scene.event_id.as_str()),
        Acknowledgement::Delivered,
        Readback::Exact,
    );
    let harmless = Upstream::serve(remote.routes()).await;
    let (model, _) = FakeModel::new(vec![vec![say("Fixture ready.")]]);
    run_event(&scene, &scene.event_id, &harmless, model).await;
    (scene, producer)
}

async fn concurrent_deliveries(
    producer: &Producer,
    scene: &crate::e2e::Scenario,
) -> Vec<serde_json::Value> {
    let barrier = tokio::sync::Barrier::new(100);
    let in_flight = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let requests = (0..100).map(|_| async {
        barrier.wait().await;
        let count = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(count, Ordering::SeqCst);
        let receipt = producer.deliver(scene).await;
        in_flight.fetch_sub(1, Ordering::SeqCst);
        receipt
    });
    let receipts = futures_util::future::join_all(requests).await;
    assert!(peak.load(Ordering::SeqCst) > 1);
    receipts
}

async fn assert_one_execution(scene: &crate::e2e::Scenario, event: &Uuid7) {
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("the scoped pool");
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.events WHERE fleet_id=$1 AND event_id=$2")
            .bind(&scene.fleet)
            .bind(event)
            .fetch_one(&mut *connection)
            .await
            .expect("the event count");
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs l JOIN fleet.events e ON e.event_id=l.event_id WHERE e.fleet_id=$1 AND e.event_id=$2")
        .bind(&scene.fleet).bind(event).fetch_one(&mut *connection).await.expect("the run count");
    assert_eq!((events, runs), (1, 1));
}
