//! The loop's order, proven against a scripted conversation.
//!
//! Each case is a way a person's run goes wrong: a message sent while the
//! model streams, a correction sent while it writes its answer, a stop in the
//! middle of a tool group, a window that cannot be fitted, a model that says
//! nothing. The last case throws
//! seeded interleavings at the loop and checks the one invariant: an answer
//! never ignores input that arrived before it.

mod call_ref;
mod extension;
mod fixture;
mod heard;
mod observe;
mod question;
mod quiet;
mod recovery;
mod script;

use serde_json::Value;

use crate::extension::Plain;
use crate::step::Ending;

use self::fixture::{FITTED_LIMIT, Scripted, answered};
use self::script::{Cue, Failure, answer, called, calling, naming, result, said, user};
use super::{ANSWER_JOIN, drive};

#[tokio::test]
async fn a_run_with_no_input_answers_after_its_one_step() {
    let mut run = Scripted::new([answer("Deployed.")]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(answered(&ending), Some("Deployed."));
    assert_eq!(run.sent().len(), 1);
}

#[tokio::test]
async fn a_message_sent_mid_stream_is_read_by_the_next_call_and_never_cuts_this_one() {
    // "Actually, use staging" arrives while call 1 streams its tool call.
    let mut run = Scripted::new([calling(1), answer("Deployed to staging.")]);
    run.during.insert(1, vec!["actually, use staging"]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(answered(&ending), Some("Deployed to staging."));
    assert!(
        !run.call_read(0, "staging"),
        "call 1 had its request already"
    );
    assert!(run.call_read(1, "staging"), "call 2 reads the correction");
}

#[tokio::test]
async fn a_correction_sent_while_the_answer_is_written_runs_one_more_step() {
    let mut run = Scripted::new([
        answer("Deployed to prod."),
        answer("Rolled back; staging now."),
    ]);
    run.during.insert(1, vec!["wait, staging not prod"]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(
        answered(&ending),
        Some(
            ["Deployed to prod.", "Rolled back; staging now."]
                .join(ANSWER_JOIN)
                .as_str()
        ),
        "the reply carries both answers, in order"
    );
}

#[tokio::test]
async fn a_correction_sent_while_an_answer_is_written_comes_after_that_answer() {
    let mut run = Scripted::new([
        answer("Deployed to prod."),
        answer("Rolled back; staging now."),
    ]);
    run.during.insert(1, vec!["wait, staging not prod"]);
    drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(
        run.sent().get(1),
        Some(&vec![
            user("deploy the fix"),
            said("Deployed to prod."),
            user("wait, staging not prod"),
        ]),
        "call 2 reads the answer, then the correction that ends the request"
    );
    assert_eq!(
        run.sent().first(),
        Some(&vec![user("deploy the fix")]),
        "the message call 1 was sent is not edited"
    );
}

#[tokio::test]
async fn a_correction_sent_while_an_answer_after_tools_is_written_comes_after_that_answer() {
    let mut run = Scripted::new([
        calling(1),
        answer("Deployed to prod."),
        answer("Rolled back; staging now."),
    ]);
    run.during.insert(2, vec!["wait, staging not prod"]);
    drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(
        run.sent().get(2),
        Some(&vec![
            user("deploy the fix"),
            called(1, 1),
            result("call-1-0"),
            said("Deployed to prod."),
            user("wait, staging not prod"),
        ]),
        "call 3 reads the answer, then the correction that ends the request"
    );
}

#[tokio::test]
async fn a_blank_answer_after_a_correction_adds_nothing_to_the_reply() {
    let mut run = Scripted::new([answer("Deployed to prod."), answer("  ")]);
    run.during.insert(1, vec!["wait, staging not prod"]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(answered(&ending), Some("Deployed to prod."));
}

#[tokio::test]
async fn messages_sent_while_tools_run_are_read_by_the_next_call() {
    let mut run = Scripted::new([calling(2), answer("Done.")]);
    run.during_calls
        .insert(1, vec!["also bump the version", "and tag it"]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(answered(&ending), Some("Done."));
    assert!(run.call_read(1, "also bump the version"));
    assert!(run.call_read(1, "and tag it"));
}

#[tokio::test]
async fn a_model_that_fails_ends_the_run_failed_and_leaves_late_input_unread() {
    let mut run = Scripted::new([calling(1), Cue::Fail]);
    run.during.insert(2, vec!["are you there?"]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert!(
        matches!(ending, Ending::Failed(Failure::Model)),
        "{ending:?}"
    );
    assert_eq!(run.pending(), ["are you there?"], "it arrived and waits");
    assert!(
        !run.messages().iter().any(|message| {
            matches!(message, crate::Message::User(said) if said.contains("are you there?"))
        }),
        "input is taken before each call, not after a failure"
    );
}

#[tokio::test]
async fn a_stop_during_the_tools_keeps_their_results_so_far_and_ends_stopped() {
    let mut run = Scripted::new([calling(3), answer("unreachable")]);
    run.stop_after_calls = Some(1);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert_eq!(run.results(), 1, "the one call that ran is remembered");
    assert_eq!(run.sent().len(), 1, "no model call follows a stop");
}

#[tokio::test]
async fn a_stop_during_the_model_call_ends_stopped() {
    let mut run = Scripted::new([Cue::Stop]);
    assert!(matches!(
        drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await,
        Ending::Stopped
    ));
}

#[tokio::test]
async fn a_window_that_cannot_be_fitted_fails_the_run_before_any_call() {
    let mut run = Scripted::new([answer("unreachable")]);
    run.window_fails = true;
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert!(
        matches!(ending, Ending::Failed(Failure::Window)),
        "{ending:?}"
    );
    assert_eq!(run.sent().len(), 0, "no call is made");
}

#[tokio::test]
async fn each_call_sends_the_window_fitted_for_it_and_the_transcript_keeps_every_message() {
    let mut run = Scripted::new([calling(1), answer("Deployed.")]);
    run.fitted = Some(1);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(answered(&ending), Some("Deployed."));
    assert_eq!(
        run.sent(),
        vec![vec![user("deploy the fix")], vec![result("call-1-0")]],
        "each call sends only what its window kept"
    );
    assert_eq!(run.limits(), vec![Some(FITTED_LIMIT); 2]);
    assert_eq!(
        run.messages().to_vec(),
        vec![
            user("deploy the fix"),
            called(1, 1),
            result("call-1-0"),
            said("Deployed."),
        ],
        "fitting a window edits nothing the loop keeps"
    );
}

#[tokio::test]
async fn a_blank_first_step_is_a_failure_not_an_answer() {
    let mut run = Scripted::new([answer("   ")]);
    assert!(matches!(
        drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await,
        Ending::Failed(Failure::Silent)
    ));
}

#[tokio::test]
async fn a_blank_answer_after_work_is_an_answer() {
    let mut run = Scripted::new([calling(1), answer("")]);
    assert_eq!(
        answered(&drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await),
        Some("")
    );
}

/// The model calls `ghost`, which the run holds no tool for: the call reads
/// the harness's refusal through the run's `Tools`, and the loop goes on.
#[tokio::test]
async fn a_call_naming_no_offered_tool_answers_not_offered() {
    let mut run = Scripted::new([naming([("ghost", Value::Null)]), answer("No ghost here.")]);
    let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
    assert_eq!(
        answered(&ending),
        Some("No ghost here."),
        "the loop goes on"
    );
    let refused = crate::Message::ToolResult {
        call_id: "call-1-0".to_owned(),
        output: "[tool_not_offered] ghost is not one of this run's tools".to_owned(),
        image: None,
    };
    assert_eq!(
        run.sent().get(1).and_then(|messages| messages.last()),
        Some(&refused),
        "the next model call reads the refusal as the call's result"
    );
    assert!(
        run.heard()
            .iter()
            .any(|line| line == "call call-1-0 Failed"),
        "{:?}",
        run.heard()
    );
}

/// A small deterministic generator, so the interleavings repeat exactly.
struct Seeded(u64);

impl Seeded {
    /// The next value below `bound`, which is small.
    fn below(&mut self, bound: u16) -> u16 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u16::try_from((self.0 >> 33) % u64::from(bound)).unwrap_or(0)
    }
}

/// Seeded runs of tool groups and answers, with input arriving mid-call,
/// mid-tools or not at all, and an occasional failure or stop. Whatever the
/// order, an answer is never given over input that arrived before it: every
/// arrival is in a user message the last model call read, or the run did not
/// answer.
#[tokio::test]
async fn no_answer_ignores_input_that_arrived_before_it() {
    const SAYINGS: [&str; 4] = ["first", "second", "third", "fourth"];
    for seed in 0..200 {
        let mut dice = Seeded(seed);
        let count = 1 + dice.below(5);
        let mut steps: Vec<Cue> = (1..count)
            .map(|_| calling(1 + usize::from(dice.below(2))))
            .collect();
        // Enough answers that every arrival can be read: one per arrival, plus one.
        steps.extend((0..=SAYINGS.len()).map(|_| answer("Done.")));
        match dice.below(10) {
            0 => steps.insert(usize::from(dice.below(count)), Cue::Fail),
            1 => steps.insert(usize::from(dice.below(count)), Cue::Stop),
            _ => {}
        }
        let mut run = Scripted::new(steps);
        for saying in SAYINGS {
            let at = u64::from(1 + dice.below(count + 1));
            match dice.below(3) {
                0 => run.during.entry(at).or_default().push(saying),
                1 => run.during_calls.entry(at).or_default().push(saying),
                _ => {}
            }
        }
        let ending = drive(&mut run, &mut Plain, &mut crate::DriveState::default()).await;
        if answered(&ending).is_none() {
            continue;
        }
        let last = run.sent().len() - 1;
        for saying in run.arrived() {
            assert!(
                run.call_read(last, saying),
                "seed {seed}: the answer ignored {saying:?}, which arrived before it"
            );
        }
    }
}
