//! Setup on the runner, end to end: a draft's turn is assigned to the Rust
//! runner as a draft-owned run, answered by its own `guffbolt` loop with
//! `propose_setup` alone, and its report writes the draft's next candidate,
//! through the daemon that ships.
//!
//! The model is a [`FakeModel`] that answers only requests carrying this
//! case's draft marker, so a stray run on the shared lane reads nothing of
//! this case's script. The scenario's own fleet is retired first, so the
//! runner's one worker takes the draft's turns alone.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::money::NANOS_PER_USD;
use afd_setup::types::MessageState;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::fake_model::{Asked, call, say};

mod case;
mod context;
mod seat;

use self::case::Case;

/// A window no setup turn here fills.
const WIDE: i32 = 128_000;
/// The most a step of the wide model writes.
const OUTPUT: u32 = 1_024;
/// What the model answers once its proposal was kept.
const ANSWER: &str = "A daily check at nine, UTC. Save it when it looks right.";
/// The tool every setup run offers, and the only one.
const PROPOSE_SETUP: &str = "propose_setup";

/// A first setup whose one cron trigger fires on `schedule`.
fn initialize(schedule: &str) -> Value {
    json!({"proposal": {"kind": "initialize", "definition": {
        "schema_version": 1,
        "description": "Checks the support queue.",
        "instructions_md": "Triage the support queue.",
        "config": {"name": "support-triage", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": schedule, "timezone": "UTC",
                "message": "Daily check"}],
            "tools": [],
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    }}})
}

/// The schedule of the one cron trigger `definition` holds.
fn schedule(definition: &Value) -> Option<&str> {
    definition
        .pointer("/config/x-agentsfleet/triggers/0/schedule")
        .and_then(Value::as_str)
}

/// Every request of this case's draft the model was sent since the last read.
fn mine<'a>(asked: &'a [Asked], marker: &str) -> Vec<&'a Asked> {
    asked
        .iter()
        .filter(|request| request.user.iter().any(|text| text.contains(marker)))
        .collect()
}

/// What the model does with a request: a request for a first version is
/// proposed once, then answered; anything else gets a question back.
fn first_version(asked: &Asked) -> Vec<guffbolt::Chunk> {
    let wants_a_version = asked
        .user
        .last()
        .is_some_and(|text| text.contains("first version"));
    match (wants_a_version, asked.results.is_empty()) {
        (true, true) => vec![call("p1", PROPOSE_SETUP, initialize("0 9 * * *"))],
        (true, false) => vec![say(ANSWER)],
        (false, _) => vec![say("Which queue should it watch, and how often?")],
    }
}

/// The person asks for a first version on their second message: the turn is
/// answered, and the draft gains candidate revision 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_answer_writes_the_next_candidate() {
    let mut supervisor = Supervisor::new();
    let (case, transcript) = Case::start(&mut supervisor, (WIDE, OUTPUT), first_version).await;
    let draft = case.seat.start(&case.said("I want a support fleet.")).await;
    let first = case.settled(&draft.id, 1).await;
    assert_eq!(first.status, MessageState::Completed.as_str(), "{first:?}");
    assert_eq!(
        case.candidates(&draft.id).await.0,
        0,
        "a question wrote a candidate"
    );

    let asking = case.said("Make a first version that checks every morning at 9.");
    case.seat.send(&draft, &asking).await;
    let second = case.settled(&draft.id, 2).await;

    assert_eq!(
        second.status,
        MessageState::Completed.as_str(),
        "{second:?}"
    );
    assert_eq!(second.answer.as_deref(), Some(ANSWER));
    let (count, head) = case.candidates(&draft.id).await;
    let (revision, definition) = head.expect("the draft has a head candidate");
    assert_eq!((count, revision), (1, 1));
    assert_eq!(schedule(&definition), Some("0 9 * * *"));
    assert!(second.candidate.is_some(), "the turn names its candidate");
    let asked = transcript.asked();
    for request in mine(&asked, &case.marker) {
        assert_eq!(
            request.tools,
            [PROPOSE_SETUP],
            "a setup run offers one tool"
        );
    }
    case.finish(supervisor, &draft).await;
}

/// A model that answers nothing fails the turn: no answer, no candidate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_whose_model_answers_nothing_fails_the_turn() {
    let mut supervisor = Supervisor::new();
    let (case, _transcript) =
        Case::start(&mut supervisor, (WIDE, OUTPUT), |_asked| Vec::new()).await;
    let draft = case.seat.start(&case.said("Make a first version.")).await;

    let turn = case.settled(&draft.id, 1).await;

    assert_eq!(turn.status, MessageState::Failed.as_str(), "{turn:?}");
    assert_eq!(turn.answer, None);
    assert!(turn.failure_reason.is_some(), "{turn:?}");
    assert_eq!(case.candidates(&draft.id).await.0, 0);
    case.finish(supervisor, &draft).await;
}

/// A setup run builds no sandbox, and still answers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_builds_no_sandbox() {
    let mut supervisor = Supervisor::new();
    let (case, _transcript) =
        Case::start(&mut supervisor, (WIDE, OUTPUT), |_asked| vec![say(ANSWER)]).await;
    let draft = case.seat.start(&case.said("What can a fleet do?")).await;

    let turn = case.settled(&draft.id, 1).await;

    assert_eq!(turn.status, MessageState::Completed.as_str(), "{turn:?}");
    assert_eq!(turn.answer.as_deref(), Some(ANSWER));
    assert_eq!(
        case.runs(&draft.id).await.len(),
        1,
        "one run answered the turn"
    );
    assert_eq!(case.prepared(), 0, "a setup run prepared a sandbox");
    case.finish(supervisor, &draft).await;
}

/// A setup run's model that calls a sandbox tool reads that it was never
/// offered, and the run still builds no sandbox.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_calling_a_sandbox_tool_still_builds_no_sandbox() {
    let mut supervisor = Supervisor::new();
    let (case, transcript) = Case::start(&mut supervisor, (WIDE, OUTPUT), |asked| {
        if asked.results.is_empty() {
            vec![call("s1", "shell", json!({"command": "ls"}))]
        } else {
            vec![say(ANSWER)]
        }
    })
    .await;
    let draft = case.seat.start(&case.said("List the files.")).await;

    let turn = case.settled(&draft.id, 1).await;

    assert_eq!(turn.status, MessageState::Completed.as_str(), "{turn:?}");
    assert_eq!(case.prepared(), 0, "a setup run prepared a sandbox");
    let asked = transcript.asked();
    let refused = mine(&asked, &case.marker)
        .into_iter()
        .find_map(|request| request.results.first().cloned())
        .expect("the shell call has a result");
    assert!(refused.starts_with("[tool_not_offered]"), "{refused}");
    case.finish(supervisor, &draft).await;
}

/// A refused proposal returns to the model, which corrects it in the same
/// run: the report carries only the corrected one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_refused_proposal_corrected_in_the_same_run_is_reported() {
    let mut supervisor = Supervisor::new();
    let (case, transcript) = Case::start(&mut supervisor, (WIDE, OUTPUT), |asked| {
        match asked.results.as_slice() {
            [] => vec![call("p1", PROPOSE_SETUP, initialize("61 * * * *"))],
            [refused] if refused.starts_with("[invalid_arguments]") => {
                vec![call("p2", PROPOSE_SETUP, initialize("0 9 * * *"))]
            }
            _ => vec![say(ANSWER)],
        }
    })
    .await;
    let draft = case
        .seat
        .start(&case.said("Check the queue every morning at 9."))
        .await;

    let turn = case.settled(&draft.id, 1).await;

    assert_eq!(turn.status, MessageState::Completed.as_str(), "{turn:?}");
    let (count, head) = case.candidates(&draft.id).await;
    assert_eq!(count, 1, "only the corrected proposal is written");
    let (_, definition) = head.expect("the draft has a head candidate");
    assert_eq!(schedule(&definition), Some("0 9 * * *"));
    let asked = transcript.asked();
    let correction = mine(&asked, &case.marker)
        .into_iter()
        .find(|request| request.results.len() == 1)
        .expect("the model read the refusal");
    assert!(
        correction
            .results
            .iter()
            .any(|result| result.contains("`schedule`")),
        "{:?}",
        correction.results
    );
    case.finish(supervisor, &draft).await;
}

/// A run whose every proposal is refused writes no candidate; its turn is
/// still answered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_whose_every_proposal_is_refused_writes_no_candidate() {
    let mut supervisor = Supervisor::new();
    let (case, _transcript) = Case::start(&mut supervisor, (WIDE, OUTPUT), |asked| {
        if asked.results.is_empty() {
            vec![call("p1", PROPOSE_SETUP, initialize("61 * * * *"))]
        } else {
            vec![say("That schedule is not valid. Which time do you want?")]
        }
    })
    .await;
    let draft = case
        .seat
        .start(&case.said("Check the queue at minute 61."))
        .await;

    let turn = case.settled(&draft.id, 1).await;

    assert_eq!(turn.status, MessageState::Completed.as_str(), "{turn:?}");
    assert!(turn.answer.is_some(), "{turn:?}");
    assert_eq!(case.candidates(&draft.id).await.0, 0);
    case.finish(supervisor, &draft).await;
}

/// The message the 200 KiB case sends, past the 32 KiB a setup message was
/// once held to.
const LONG_MESSAGE_BYTES: usize = 200 * 1024;

/// A 200 KiB message is stored whole, reaches the setup run's model whole, and
/// is answered; the one other outcome the ceiling leaves is the model's own
/// context refusal, and that is the only failure this accepts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_message_over_32_kib_is_answered() {
    let mut supervisor = Supervisor::new();
    let (case, transcript) =
        Case::start(&mut supervisor, (WIDE, OUTPUT), |_asked| vec![say(ANSWER)]).await;
    let line = "Watch the support queue and page on-call when it backs up.\n";
    let long = case.said(&line.repeat(LONG_MESSAGE_BYTES / line.len() + 1));
    assert!(long.len() > LONG_MESSAGE_BYTES, "{}", long.len());
    let draft = case.seat.start(&long).await;

    let stored: String = sqlx::query_scalar(
        "SELECT request_json->>'message' FROM thread.turns WHERE thread_id = $1 AND sequence = 1",
    )
    .bind(&draft.id)
    .fetch_one(&mut *case.connection().await)
    .await
    .expect("the message is stored");
    assert_eq!(stored, long, "the message is stored whole");
    let turn = case.settled(&draft.id, 1).await;

    if turn.status == MessageState::Completed.as_str() {
        assert_eq!(turn.answer.as_deref(), Some(ANSWER), "{turn:?}");
        let asked = transcript.asked();
        let reached = mine(&asked, &case.marker)
            .into_iter()
            .any(|request| request.user.iter().any(|text| text.contains(&long)));
        assert!(reached, "the setup run's model read the whole message");
    } else {
        assert_eq!(
            (turn.status.as_str(), turn.failure_reason.as_deref()),
            (MessageState::Failed.as_str(), Some("context_too_large")),
            "{turn:?}"
        );
    }
    case.finish(supervisor, &draft).await;
}
