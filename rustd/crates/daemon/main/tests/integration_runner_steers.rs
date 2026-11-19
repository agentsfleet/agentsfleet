//! A running fleet puts work on another fleet in its workspace,
//! once per text, under its own actor, in the thread it was asked from.
//!
//! The refusals are in `integration_runner_steers_refusals`, the busy target
//! and the chain in `integration_runner_steers_chain`, the fence in
//! `integration_runner_steers_fence`.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use af_wire::event::{FleetSteerBody, STEER_MESSAGE_MAX_BYTES};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Barrier;

use crate::definition_install::seal;
use crate::e2e_seed::FLEET_NAME;
use crate::offers::{offered, settle, steered};
use crate::steers::Crew;
use crate::verbs::enqueue_from_thread;
use crate::wire::{report, report_body, wait_for_offer};

/// How many identical steers race for one admission, released together.
const RACERS: usize = 100;

/// How many different steers one run sends in a row.
const STEPS: usize = 20;

/// A sender fleet that declares the `grafana` credential, for the mask.
const DECLARING: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":[],"credentials":["grafana"],"budget":{"daily_nanos":"1000000000"}}}"#;

/// The secret value the mask case's sender holds.
const TOKEN: &str = "glsa_fixture_steer_secret";

/// The body a steer of `text` from the scenario's seeded event stores.
fn expected_body(crew: &Crew, text: &str) -> Value {
    let scene = crew.run();
    let body = FleetSteerBody::new(
        text,
        scene.fleet.as_str(),
        FLEET_NAME,
        scene.event_id.as_str(),
    );
    serde_json::to_value(&body).expect("the body serializes")
}

/// A hundred steers with one text, released together from a barrier, admit one
/// row, under the fleet's actor and the person's producer; every caller reads
/// its event, and a later resend is answered as a repeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_admits_once_under_the_fleet_actor() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let text = "scene the hourly check";

    let barrier = Barrier::new(RACERS);
    let (inflight, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let answers = futures_util::future::join_all((0..RACERS).map(|_racer| async {
        barrier.wait().await;
        peak.fetch_max(
            inflight.fetch_add(1, Ordering::SeqCst) + 1,
            Ordering::SeqCst,
        );
        let answered = crew.steer((&run, fence), &target, text).await;
        inflight.fetch_sub(1, Ordering::SeqCst);
        answered
    }))
    .await;
    assert!(
        peak.load(Ordering::SeqCst) > 1,
        "the callers overlapped: peak in flight {}",
        peak.load(Ordering::SeqCst)
    );
    let rows = crew.admissions(&target).await;
    assert_eq!(rows.len(), 1, "{answers:?}");
    let row = &rows[0];
    assert_eq!(row.producer, "steer");
    assert_eq!(row.actor, format!("fleet:{}", crew.run().fleet));
    assert_eq!(row.event_type, "chat");
    let stored: Value = serde_json::from_str(&row.request_json).expect("the body is JSON");
    assert_eq!(stored, expected_body(&crew, text));
    for (status, answer) in &answers {
        assert_eq!(*status, 202, "{answer}");
        assert_eq!(answer["event_id"], row.id.as_str(), "{answer}");
    }
    let fresh = answers
        .iter()
        .filter(|(_, answer)| answer["replayed"] == false)
        .count();
    assert_eq!(fresh, 1, "{answers:?}");

    let (status, again) = crew.steer((&run, fence), &target, text).await;
    assert_eq!((status, &again["replayed"]), (202, &json!(true)), "{again}");
    assert_eq!(again["event_id"], row.id.as_str());
    crew.finish(supervisor).await;
}

/// A steer from a Slack thread's event answers in that thread;
/// one from an event with no thread carries none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_copies_the_sender_reply_destination() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;

    let (seeded, fence) = crew.speaking.run_seeded().await;
    let (status, _) = crew
        .steer((&seeded, fence), &target, "no thread here")
        .await;
    assert_eq!(status, 202);
    let reported = report(
        crew.run(),
        &report_body(&seeded, &crew.run().event_id, fence),
    )
    .await;
    assert!(reported.is_ok(), "{reported:?}");

    let thread_event = enqueue_from_thread(crew.run()).await;
    let (threaded, fence) = wait_for_offer(crew.run(), &thread_event).await;
    let (status, _) = crew
        .steer((&threaded, fence), &target, "from the thread")
        .await;
    assert_eq!(status, 202);

    let sender = crew.admissions(&crew.run().fleet).await;
    let asked = sender
        .iter()
        .find(|row| row.id == thread_event)
        .expect("the thread's admission");
    let rows = crew.admissions(&target).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (&rows[0].reply_provider, &rows[0].reply_address),
        (&None, &None)
    );
    assert_eq!(rows[1].reply_provider.as_deref(), Some("slack"));
    assert_eq!(rows[1].reply_address, asked.reply_address);
    crew.finish(supervisor).await;
}

/// A resend is answered with the first event before the
/// target's status is read, still one row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_replays_before_status() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;

    let (status, first) = crew.steer((&run, fence), &target, "open the fix").await;
    assert_eq!(status, 202, "{first}");
    let (status, again) = crew.steer((&run, fence), &target, "open the fix").await;
    assert_eq!((status, &again["replayed"]), (202, &json!(true)), "{again}");
    assert_eq!(again["event_id"], first["event_id"]);

    crew.alter(
        "UPDATE fleet.fleets SET status = 'archived' WHERE id = $1",
        &target,
    )
    .await;
    let (status, archived) = crew.steer((&run, fence), &target, "open the fix").await;
    assert_eq!(
        (status, &archived["event_id"]),
        (202, &first["event_id"]),
        "{archived}"
    );
    assert_eq!(crew.admissions(&target).await.len(), 1);
    crew.finish(supervisor).await;
}

/// The target's run reads the sender line, then the words,
/// at a short text and at the bound; and the next run reads it as a prior turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_body_reaches_the_target_turn() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.running_sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let long = "w".repeat(STEER_MESSAGE_MAX_BYTES);
    let (_, short_answer) = crew
        .steer((&run, fence), &target, "scene the hourly check")
        .await;
    let (_, long_answer) = crew.steer((&run, fence), &target, &long).await;
    let reported = report(crew.run(), &report_body(&run, &crew.run().event_id, fence)).await;
    assert!(reported.is_ok(), "{reported:?}");

    let line = af_wire::event::sender_line(FLEET_NAME, crew.run().fleet.as_str());
    for (answer, text) in [
        (&short_answer, "scene the hourly check"),
        (&long_answer, long.as_str()),
    ] {
        let turn = offered(crew.run(), &steered(answer)).await;
        assert_eq!(turn.message, format!("{line}\n{text}"));
        settle(crew.run(), &turn).await;
    }
    let next = crate::e2e_event::enqueue(
        &crew.run().booted,
        &target,
        &crew.run().workspace,
        af_wire::event::EventType::Chat,
    )
    .await;
    let turn = offered(crew.run(), &next).await;
    assert!(
        turn.history
            .iter()
            .any(|message| message.starts_with(&line)),
        "a prior turn reads the sender line: {:?}",
        turn.history
    );
    settle(crew.run(), &turn).await;
    crew.finish(supervisor).await;
}

/// A value of the sender's stored secret never reaches the
/// stored body.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_masks_secret_values() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot_with(&mut supervisor, DECLARING).await;
    let secret = json!({"host": "grafana.example.test", "token": TOKEN}).to_string();
    seal(
        &crew.run().booted,
        &crew.run().workspace,
        "grafana",
        &secret,
        af_core::clock::now(),
    )
    .await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;

    let (status, _) = crew
        .steer((&run, fence), &target, &format!("use {TOKEN}"))
        .await;
    assert_eq!(status, 202);
    let rows = crew.admissions(&target).await;
    assert!(
        !rows[0].request_json.contains(TOKEN),
        "{}",
        rows[0].request_json
    );
    let stored: Value = serde_json::from_str(&rows[0].request_json).expect("JSON");
    assert_eq!(stored["text"], "use «secret:grafana.token»");
    crew.finish(supervisor).await;
}

/// Nothing caps how many steers one run sends.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_has_no_per_run_limit() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;

    for step in 0..STEPS {
        let (status, answer) = crew
            .steer((&run, fence), &target, &format!("step {step}"))
            .await;
        assert_eq!(
            (status, &answer["replayed"]),
            (202, &json!(false)),
            "{answer}"
        );
    }
    assert_eq!(crew.admissions(&target).await.len(), STEPS);
    crew.finish(supervisor).await;
}
