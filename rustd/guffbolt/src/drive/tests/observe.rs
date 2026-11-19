//! What the loop tells its observer: each model call and each tool call from
//! its start to its end, in the order they happen, and every call ended once
//! on every path a loop can take.

use std::time::Duration;

use crate::DriveState;
use crate::extension::{Budget, Extension, Plain};
use crate::transcript::Message;
use crate::turn::Ending;

use super::drive;
use super::fixture::Scripted;
use super::script::{Step, answer, calling};

/// An extension that adds only its budget.
struct Bounded(Budget);

impl Extension<Scripted> for Bounded {
    fn budget(&self, _conversation: &Scripted) -> Budget {
        self.0
    }
}

#[tokio::test]
async fn every_turn_and_call_is_heard_from_its_start_to_its_end_in_order() {
    let mut run = Scripted::new([calling(2), answer("Done.")]);
    drive(&mut run, &mut Plain, &mut DriveState::default()).await;
    assert_eq!(
        run.heard(),
        [
            "turn 1 started",
            "turn 1 read",
            "call call-1-0 started",
            "call call-1-0 Succeeded",
            "call call-1-1 started",
            "call call-1-1 Succeeded",
            "turn 2 started",
            "turn 2 read",
            "loop answered after 2",
        ]
    );
}

#[tokio::test]
async fn a_call_the_stop_cuts_short_ends_interrupted_and_the_next_never_starts() {
    let mut run = Scripted::new([calling(3), answer("unreachable")]);
    run.stop_after_calls = Some(1);
    let ending = drive(&mut run, &mut Plain, &mut DriveState::default()).await;
    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert_eq!(
        run.heard(),
        [
            "turn 1 started",
            "turn 1 read",
            "call call-1-0 started",
            "call call-1-0 Succeeded",
            "call call-1-1 started",
            "call call-1-1 Interrupted",
            "loop stopped after 1",
        ]
    );
    assert_eq!(run.results(), 1, "an interrupted call leaves no result");
}

#[tokio::test(start_paused = true)]
async fn a_call_still_running_when_the_budget_runs_out_ends_interrupted() {
    let mut run = Scripted::new([calling(1)]);
    run.calls_hang = true;
    let mut budget = Bounded(Budget::Within(Duration::from_secs(30)));
    let ending = drive(&mut run, &mut budget, &mut DriveState::default()).await;
    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert_eq!(
        run.heard(),
        [
            "turn 1 started",
            "turn 1 read",
            "call call-1-0 started",
            "call call-1-0 Interrupted",
            "loop stopped after 1",
        ]
    );
}

#[tokio::test]
async fn a_cut_turns_call_ends_failed_without_running() {
    let mut run = Scripted::new([Step::Cut(1), answer("Shorter, then.")]);
    drive(&mut run, &mut Plain, &mut DriveState::default()).await;
    let heard = run.heard();
    assert!(
        heard.iter().any(|line| line == "call call-1-0 Failed"),
        "{heard:?}"
    );
    let refused = run.messages().iter().any(|message| {
        matches!(message, Message::ToolResult { output, .. }
            if output.starts_with("[output_limit_reached] deploy "))
    });
    assert!(refused, "the model reads why its call never ran");
}

#[tokio::test]
async fn a_turn_that_fails_or_is_stopped_is_heard_so_and_ends_the_loop() {
    let mut failing = Scripted::new([Step::Fail]);
    drive(&mut failing, &mut Plain, &mut DriveState::default()).await;
    assert_eq!(
        failing.heard(),
        ["turn 1 started", "turn 1 failed", "loop failed after 1"]
    );

    let mut stopping = Scripted::new([Step::Stop]);
    drive(&mut stopping, &mut Plain, &mut DriveState::default()).await;
    assert_eq!(
        stopping.heard(),
        ["turn 1 started", "turn 1 stopped", "loop stopped after 1"]
    );
}

#[tokio::test]
async fn a_loop_its_budget_never_lets_start_still_ends_once() {
    let mut run = Scripted::new([answer("unreachable")]);
    let mut spent = Bounded(Budget::Spent);
    drive(&mut run, &mut spent, &mut DriveState::default()).await;
    assert_eq!(run.heard(), ["loop stopped after 0"]);
    assert_eq!(run.sent().len(), 0, "no model call is made");
}

#[tokio::test]
async fn the_model_reads_a_calls_output_as_the_observer_left_it() {
    let mut run = Scripted::new([calling(1), answer("Done.")]);
    run.mask = Some("«masked»");
    drive(&mut run, &mut Plain, &mut DriveState::default()).await;
    assert_eq!(
        run.sent().get(1).and_then(|messages| messages.last()),
        Some(&Message::ToolResult {
            call_id: "call-1-0".to_owned(),
            output: "«masked»".to_owned(),
            image: None,
        }),
        "the next call reads the masked output"
    );
}
