//! A question for the person: the run ends waiting on the call that asked,
//! a resumed run reads the answer as that call's result, an answer with no
//! question waiting changes nothing, and one step asks one question.

use crate::step::{Ending, Waiting};
use crate::transcript::{Message, Replay};
use crate::{DriveState, Plain};

use super::drive;
use super::fixture::{Scripted, answered};
use super::script::{answer, asking, call_to, naming, user};

/// The question the scripted run asks.
const QUESTION: &str = "Ship to prod?";

/// The run waits on call `call_id`, which asked `question`.
fn waiting_on(call_id: &str, question: &str) -> Waiting {
    Waiting {
        call_id: call_id.to_owned(),
        question: question.to_owned(),
    }
}

/// What call `call_id` answered with `output`.
fn result_of(call_id: &str, output: &str) -> Message {
    Message::ToolResult {
        call_id: call_id.to_owned(),
        output: output.to_owned(),
        image: None,
    }
}

#[tokio::test]
async fn a_resumed_run_answers_the_question_call() {
    let (name, arguments) = asking(QUESTION);
    let asked = Message::Assistant {
        text: String::new(),
        calls: vec![call_to(1, 0, name, arguments.clone())],
        replay: Replay::default(),
    };
    let mut run = Scripted::new([naming([(name, arguments)]), answer("Shipping to prod.")]);
    let mut state = DriveState::default();

    let first = drive(&mut run, &mut Plain, &mut state).await;
    let resumed = state.resume(&mut run.transcript, "yes");
    let ending = drive(&mut run, &mut Plain, &mut state).await;

    let waiting = waiting_on("call-1-0", QUESTION);
    assert!(
        matches!(&first, Ending::Waiting(on) if *on == waiting),
        "{first:?}"
    );
    assert_eq!(resumed, Some(waiting));
    assert_eq!(answered(&ending), Some("Shipping to prod."));
    assert_eq!(
        run.sent().get(1),
        Some(&vec![
            user("deploy the fix"),
            asked,
            result_of("call-1-0", "yes")
        ]),
        "the next request holds one result for the asking call: the answer"
    );
    assert_eq!(state.waiting(), None, "the question was answered");
}

#[tokio::test]
async fn an_answer_with_no_question_waiting_changes_nothing() {
    let mut run = Scripted::new([naming([asking(QUESTION)])]);
    let mut state = DriveState::default();

    let early = state.resume(&mut run.transcript, "too early");
    let before = run.messages().to_vec();
    drive(&mut run, &mut Plain, &mut state).await;
    let first = state.resume(&mut run.transcript, "yes");
    let again = state.resume(&mut run.transcript, "no, wait");

    assert_eq!(early, None, "nothing was asked yet");
    assert_eq!(
        before,
        [user("deploy the fix")],
        "the early answer added nothing"
    );
    assert_eq!(first, Some(waiting_on("call-1-0", QUESTION)));
    assert_eq!(again, None, "the question was answered already");
    assert_eq!(run.results(), 1, "only the first answer is a result");
    assert_eq!(run.messages().last(), Some(&result_of("call-1-0", "yes")));
}

#[tokio::test]
async fn a_second_question_in_one_step_is_refused() {
    let mut run = Scripted::new([naming([asking(QUESTION), asking("Tag the release?")])]);
    let mut state = DriveState::default();

    let ending = drive(&mut run, &mut Plain, &mut state).await;

    let waiting = waiting_on("call-1-0", QUESTION);
    assert!(
        matches!(&ending, Ending::Waiting(on) if *on == waiting),
        "the first question is the one the run waits on: {ending:?}"
    );
    let refused = "[question_pending] another call of this step already asked the person a \
                   question; ask this one after the answer arrives";
    assert_eq!(
        run.messages().last(),
        Some(&result_of("call-1-1", refused)),
        "the second question reads why it was not asked"
    );
    assert_eq!(
        run.results(),
        1,
        "the first has no result until it is answered"
    );
    assert_eq!(state.waiting(), Some(&waiting));
    assert_eq!(run.sent().len(), 1, "no model call follows a question");
}
