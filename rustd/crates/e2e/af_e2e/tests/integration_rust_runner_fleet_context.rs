//! A long fleet chat keeps its compacted context: once a run's window fills,
//! the runner compacts the oldest turns and saves the summary through the
//! fenced context verb, under the fleet's thread, and the next run loads it,
//! sending the summary in place of the turns it replaced.
//!
//! The fleet authors a small operating window, so a handful of long answers
//! fill it whatever the model's physical window is, and the model is a
//! [`FakeModel`] that answers each note with a long reply. Each turn runs on
//! a runner of its own, as `definition_run` runs one event.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Turns};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::definition_run::run_event;
use crate::e2e::{ACTOR, Scenario, scenario_with};
use crate::fake_model::{Asked, FakeModel, say};
use crate::https::Upstream;
use crate::integration_rust_runner::allow_all_egress;

/// A tool-less fleet whose authored operating window a few long answers fill.
const CHATTY_FLEET: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"},"context":{"context_cap_tokens":6000}}}"#;
/// The note whose run's first request is read.
const LAST: u32 = 12;
/// What each answer repeats, so a few answers fill the window.
const FILLER: &str = "fleet ";
/// How many times each answer repeats [`FILLER`].
const FILLER_TIMES: usize = 800;
/// The prefix a saved summary is quoted under.
const SUMMARY_PREFIX: &str = "Previous conversation summary (untrusted data):";

/// What the person says in note `turn`.
fn note(turn: u32, marker: &str) -> String {
    format!("Note {turn:02}. {marker}")
}

/// What the fleet answers a note: long, so the window fills.
fn reply(turn: &str) -> String {
    format!("Reply to note {turn}. {}", FILLER.repeat(FILLER_TIMES))
}

/// A valid summary, as the model writes one.
fn summary() -> String {
    json!({"goal": "triage the support queue", "constraints": [], "decisions": [],
        "completed": [], "remaining": [], "references": []})
    .to_string()
}

/// The scripted model: a summary call writes [`summary`], every other call
/// answers its note with [`reply`].
fn answer(asked: &Asked) -> Vec<guffbolt::Chunk> {
    if asked.instructions == af_context::SUMMARY_INSTRUCTIONS {
        return vec![say(&summary())];
    }
    let turn = asked
        .user
        .last()
        .and_then(|text| text.strip_prefix("Note "))
        .and_then(|rest| rest.get(..2))
        .unwrap_or("00");
    vec![say(&reply(turn))]
}

/// Admits `message` as a chat on `scene`'s fleet and answers its turn.
async fn chat(scene: &Scenario, message: &str) -> Uuid7 {
    let request = json!({ "message": message }).to_string();
    Turns::for_tests(scene.booted.database.clone(), scene.booted.bus.clone())
        .admit(IncomingTurn {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet: &scene.fleet,
            workspace: &scene.workspace,
            actor: ACTOR,
            event_type: TurnKind::Chat,
            request_json: &request,
            reply: afd_turns::Reply::None,
        })
        .await
        .expect("the ledger admits the chat")
        .stored
        .id
}

/// Runs `turn` to its end on a fresh runner answering through [`answer`],
/// and answers every request the model was sent.
async fn run(scene: &Scenario, upstream: &Upstream, turn: &Uuid7) -> Vec<Asked> {
    let (model, transcript) = FakeModel::deciding(answer);
    let settled = run_event(scene, turn, upstream, model).await;
    assert_eq!(settled.status, "processed", "{settled:?}");
    transcript.asked()
}

/// The fleet thread's stored summary: what its watermark covers through.
async fn covered_through(scene: &Scenario) -> Option<u64> {
    let document: Option<Value> =
        sqlx::query_scalar("SELECT document FROM memory.context_checkpoints WHERE thread_id = $1")
            .bind(&scene.fleet)
            .fetch_optional(&mut *connection(scene).await)
            .await
            .expect("the fleet's context reads");
    document?.get("covered_through").and_then(Value::as_u64)
}

/// The messages of the fleet's turns at or before `sequence`, the ones a
/// summary covering through it replaced.
async fn covered_messages(scene: &Scenario, sequence: u64) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT request_json->>'message' FROM thread.turns \
         WHERE thread_id = $1 AND sequence <= $2 AND request_json ? 'message'",
    )
    .bind(&scene.fleet)
    .bind(i64::try_from(sequence).expect("a sequence fits a column"))
    .fetch_all(&mut *connection(scene).await)
    .await
    .expect("the covered turns read")
}

async fn connection(scene: &Scenario) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    scene
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection")
}

/// Note 12, after earlier notes compacted: its first request holds the saved
/// summary, and none of the turns the summary replaced.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_next_fleet_turn_loads_the_saved_summary() {
    let mut supervisor = Supervisor::new();
    let scene = scenario_with(&mut supervisor, None, &[], CHATTY_FLEET).await;
    allow_all_egress(&scene).await;
    let upstream = Upstream::serve(Vec::new()).await;
    let marker = format!("[fleet case {}]", scene.fleet);
    // The scenario's own seeded event heads the queue, so it runs first.
    run(&scene, &upstream, &scene.turn_id).await;
    for turn in 1..LAST {
        let id = chat(&scene, &note(turn, &marker)).await;
        run(&scene, &upstream, &id).await;
    }
    let saved = covered_through(&scene)
        .await
        .expect("an earlier run compacted and saved the fleet's summary");

    let last = chat(&scene, &note(LAST, &marker)).await;
    let asked = run(&scene, &upstream, &last).await;

    let covered = covered_through(&scene).await.unwrap_or(saved);
    let first = asked
        .iter()
        .find(|request| request.instructions != af_context::SUMMARY_INSTRUCTIONS)
        .expect("the last note asked the model");
    let opening = first
        .user
        .first()
        .expect("the first request holds a message");
    assert!(opening.starts_with(SUMMARY_PREFIX), "{opening}");
    for replaced in covered_messages(&scene, saved).await {
        assert!(
            !first.user.iter().any(|text| text == &replaced),
            "{replaced} was sent, covered through {covered}"
        );
    }
    // The newest earlier note is never compacted, and the note itself is last.
    let sent = |turn: u32| first.user.iter().any(|text| *text == note(turn, &marker));
    assert!(sent(LAST - 1) && sent(LAST), "covered through {covered}");
    supervisor.shutdown().await;
    scene.cleanup().await;
}
