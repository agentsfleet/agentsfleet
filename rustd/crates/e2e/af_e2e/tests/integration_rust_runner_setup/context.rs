//! A long setup keeps its compacted context: once a turn's window fills, the
//! runner compacts the oldest turns and saves the summary through the fenced
//! context verb, and a later turn loads it, sending the summary in place of
//! the turns it replaced.
//!
//! The window is sized from the request the runner builds, so the oldest
//! turns no longer fit after four long answers whatever the tool schema
//! weighs: every later turn reads the saved summary, and a compaction runs
//! every second turn.

use af_fleet_runtime::setup::{DESCRIPTION, NAME, parameters};
use afd_setup::types::MessageState;
use agentsfleetd::supervisor::Supervisor;
use guffbolt::{Message, Replay, Request, ToolSpec};
use serde_json::{Value, json};

use super::case::Case;
use crate::fake_model::{Asked, say};

/// The turn whose first request is read.
const LAST: u32 = 20;
/// The most a step of the narrow model writes.
const OUTPUT: u32 = 256;
/// What each answer repeats, long enough that four answers fill the window
/// and short of the per-turn history cut.
const FILLER: &str = "setup ";
/// How many times each answer repeats [`FILLER`].
const FILLER_TIMES: usize = 2_400;
/// The prefix a saved summary is quoted under.
const SUMMARY_PREFIX: &str = "Previous conversation summary (untrusted data):";
/// A model name of the seated model's family, which picks the tokenizer the
/// window is measured with.
const MEASURED_AS: &str = "gpt-setup";
/// A marker as long as a case's, to measure the window before the case's
/// scenario exists.
const STAND_IN: &str = "[draft case 0195b4ba-8d3a-7000-8abc-000000000000]";

/// What the person says on turn `turn`.
fn note(turn: u32, marker: &str) -> String {
    format!("Note {turn:02}. {marker}")
}

/// What setup answers turn `turn`: long, so the window fills.
fn reply(turn: &str) -> String {
    format!("Reply to note {turn}. {}", FILLER.repeat(FILLER_TIMES))
}

/// A valid summary, as the model writes one.
fn summary() -> String {
    json!({"goal": "a support fleet", "constraints": [], "decisions": [], "completed": [],
        "remaining": [], "references": []})
    .to_string()
}

/// The input a setup request of `history` and `current` costs, as the
/// runner's scripted model measures it.
fn cost(history: &[(String, String)], current: &str) -> u64 {
    let schema = parameters(false);
    let tools = [ToolSpec {
        name: NAME,
        description: DESCRIPTION,
        parameters: &schema,
    }];
    let tool = afd_setup::setup_run::setup_tool(false);
    let instructions = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut messages: Vec<Message> = history
        .iter()
        .flat_map(|(asked, answered)| {
            [
                Message::User(asked.clone()),
                Message::Assistant {
                    text: answered.clone(),
                    calls: Vec::new(),
                    replay: Replay::default(),
                },
            ]
        })
        .collect();
    messages.push(Message::User(current.to_owned()));
    af_providers::testing::measure(Request {
        model: MEASURED_AS,
        instructions,
        messages: &messages,
        tools: &tools,
        hosted: &[],
        cache_key: "",
        max_output_tokens: Some(OUTPUT),
        single_attempt: false,
    })
    .expect("a setup request measures")
}

/// A window four long answers overfill and two fit: the base request and
/// four and a half answered turns.
fn window(marker: &str) -> i32 {
    let base = cost(&[], &note(1, marker));
    let one = cost(&[(note(1, marker), reply("01"))], &note(2, marker));
    let per_turn = one.saturating_sub(base);
    let tokens = base + per_turn * 4 + per_turn / 2;
    i32::try_from(tokens).expect("the window fits a column")
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
        .unwrap_or("??");
    vec![say(&reply(turn))]
}

/// The draft's stored context: what its watermark covers through.
async fn covered_through(case: &Case, draft: &af_core::id::Uuid7) -> Option<u64> {
    let document: Option<Value> =
        sqlx::query_scalar("SELECT document FROM memory.context_checkpoints WHERE thread_id = $1")
            .bind(draft)
            .fetch_optional(&mut *case.connection().await)
            .await
            .expect("the draft's context reads");
    document?.get("covered_through").and_then(Value::as_u64)
}

/// Turn 20's first request that answers rather than summarizes.
fn first_answering<'a>(asked: &'a [Asked], marker: &str) -> &'a Asked {
    asked
        .iter()
        .find(|request| {
            request.instructions != af_context::SUMMARY_INSTRUCTIONS
                && request.user.iter().any(|text| text.contains(marker))
        })
        .expect("turn 20 asked the model")
}

/// Turn 20, after earlier turns compacted: its first request holds the saved
/// summary, and none of the turns the summary replaced.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_next_setup_turn_loads_the_saved_context() {
    let mut supervisor = Supervisor::new();
    let narrow = window(STAND_IN);
    let (case, transcript) = Case::start(&mut supervisor, (narrow, OUTPUT), answer).await;
    let draft = case.seat.start(&note(1, &case.marker)).await;
    for turn in 1..LAST {
        if turn > 1 {
            case.seat.send(&draft, &note(turn, &case.marker)).await;
        }
        let settled = case.settled(&draft.id, i64::from(turn)).await;
        assert_eq!(
            settled.status,
            MessageState::Completed.as_str(),
            "turn {turn}: {settled:?}"
        );
    }
    let saved = covered_through(&case, &draft.id)
        .await
        .expect("an earlier turn compacted and saved its context");
    // Only turn 20's requests are read below.
    drop(transcript.asked());

    case.seat.send(&draft, &note(LAST, &case.marker)).await;
    let last = case.settled(&draft.id, i64::from(LAST)).await;

    assert_eq!(last.status, MessageState::Completed.as_str(), "{last:?}");
    let covered = covered_through(&case, &draft.id).await.unwrap_or(saved);
    let asked = transcript.asked();
    let first = first_answering(&asked, &case.marker);
    let opening = first
        .user
        .first()
        .expect("the first request holds a message");
    assert!(opening.starts_with(SUMMARY_PREFIX), "{opening}");
    let sent = |turn: u32| {
        let said = note(turn, &case.marker);
        first.user.iter().any(|text| text.contains(&said))
    };
    for turn in (1..LAST).filter(|turn| u64::from(*turn) <= covered) {
        assert!(
            !sent(turn),
            "turn {turn} was sent, covered through {covered}"
        );
    }
    // The newest earlier turn is never compacted, and the turn itself is last.
    assert!(sent(LAST - 1) && sent(LAST), "covered through {covered}");
    case.finish(supervisor, &draft).await;
}
