//! Which input was newest when each step began: the loop keeps the newest
//! mark it takes, notes it as a step begins, and every call of that step ends
//! carrying it beside its reference.

use std::collections::HashMap;

use super::drive;
use super::fixture::{Scripted, answered};
use super::script::{answer, calling};
use crate::{DriveState, Mark, Plain};

/// The first message's text and the mark it arrives with.
const FIRST: (&str, &str) = ("first", "turn-1");
/// The second message's text and the mark it arrives with.
const SECOND: (&str, &str) = ("use staging", "turn-2");

/// `run` with both messages' marks.
fn marking(mut run: Scripted) -> Scripted {
    run.marks = HashMap::from([FIRST, SECOND]);
    run
}

/// Each call's step and the mark it ended with, `marks` spelled as text.
fn expected(marks: &[(u64, Option<&str>)]) -> Vec<(u64, Option<String>)> {
    marks
        .iter()
        .map(|(step, mark)| (*step, mark.map(str::to_owned)))
        .collect()
}

#[tokio::test]
async fn items_after_a_second_message_name_the_second_turn() {
    // The second message arrives while step 2's model call streams, so step 3
    // is the first step that reads it.
    let mut run = marking(Scripted::new([
        calling(1),
        calling(1),
        calling(1),
        answer("Deployed to staging."),
    ]));
    run.sends(FIRST.0);
    run.during.insert(2, vec![SECOND.0]);
    let mut state = DriveState::default();
    let ending = drive(&mut run, &mut Plain, &mut state).await;
    assert_eq!(answered(&ending), Some("Deployed to staging."));
    assert_eq!(
        run.ended_marks(),
        expected(&[
            (1, Some("turn-1")),
            (2, Some("turn-1")),
            (3, Some("turn-2"))
        ]),
        "steps 1 and 2 name the first turn, step 3 the second"
    );
    assert_eq!(state.newest().map(Mark::as_str), Some("turn-2"));
}

#[tokio::test]
async fn unmarked_input_keeps_the_newest_mark() {
    let mut run = marking(Scripted::new([calling(1), calling(1), answer("Deployed.")]));
    run.sends(FIRST.0);
    run.during.insert(1, vec!["a note no turn names"]);
    let mut state = DriveState::default();
    drive(&mut run, &mut Plain, &mut state).await;
    assert_eq!(
        run.ended_marks(),
        expected(&[(1, Some("turn-1")), (2, Some("turn-1"))]),
        "an unmarked message read by step 2 leaves the first turn newest"
    );
}

#[tokio::test]
async fn a_run_whose_input_names_nothing_ends_each_call_unmarked() {
    let mut run = Scripted::new([calling(2), answer("Deployed.")]);
    run.sends(FIRST.0);
    let mut state = DriveState::default();
    drive(&mut run, &mut Plain, &mut state).await;
    assert_eq!(run.ended_marks(), expected(&[(1, None), (1, None)]));
    assert_eq!(state.newest(), None);
}

#[tokio::test]
async fn input_taken_after_an_answer_marks_the_step_it_starts() {
    // The second message arrives while step 1 writes its answer: the loop
    // takes it after that answer, and step 2 is the one that reads it.
    let mut run = marking(Scripted::new([
        answer("Deployed to prod."),
        calling(1),
        answer("Rolled back; staging now."),
    ]));
    run.sends(FIRST.0);
    run.during.insert(1, vec![SECOND.0]);
    drive(&mut run, &mut Plain, &mut DriveState::default()).await;
    assert_eq!(run.ended_marks(), expected(&[(2, Some("turn-2"))]));
}

#[tokio::test]
async fn a_resumed_loop_keeps_the_newest_mark_it_saved() {
    let mut first = marking(Scripted::new([calling(1), answer("Deployed.")]));
    first.sends(SECOND.0);
    let mut state = DriveState::default();
    drive(&mut first, &mut Plain, &mut state).await;
    let saved = serde_json::to_string(&state).unwrap_or_default();
    let mut resumed: DriveState = serde_json::from_str(&saved).unwrap_or_default();
    // The resumed run's input carries no mark: its calls name the saved one.
    let mut second = Scripted::new([calling(1), answer("Checked.")]);
    second.sends("check it");
    drive(&mut second, &mut Plain, &mut resumed).await;
    assert_eq!(second.ended_marks(), expected(&[(3, Some("turn-2"))]));
}

#[test]
fn a_state_saved_before_marks_reads_with_none() {
    let saved = r#"{"answers":["Deployed."],"completed_steps":2}"#;
    let state: Result<DriveState, _> = serde_json::from_str(saved);
    assert_eq!(state.ok().as_ref().map(DriveState::newest), Some(None));
}

#[test]
fn a_state_naming_an_unknown_field_is_refused() {
    let saved = r#"{"answers":[],"completed_steps":0,"latest":"turn-1"}"#;
    let refused = serde_json::from_str::<DriveState>(saved).err();
    assert!(
        refused.is_some(),
        "a misspelt mark field is not read as none"
    );
}
