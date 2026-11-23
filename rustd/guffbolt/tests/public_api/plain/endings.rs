//! How a plain loop ends: a failed model, a stop, a spent budget or a window
//! that fails.

use super::*;

#[tokio::test]
async fn a_model_that_fails_ends_the_loop_failed() {
    let script = Script::default().plays(KEY, [call("note", json!({"line": "one"}))]);
    let mut chat = Chat::new(script, "go");
    let mut state = DriveState::default();

    let ending = drive(&mut chat, &mut Plain, &mut state).await;

    assert!(
        matches!(ending, Ending::Failed(Failure::Exhausted)),
        "{ending:?}"
    );
    assert_eq!(ending.kind(), "failed");
    assert_eq!(
        *lock(&chat.desk.notes),
        ["one"],
        "the step before the failure ran"
    );
    assert_eq!(state.answers(), Vec::<String>::new());
}

#[tokio::test]
async fn a_blank_first_step_fails_as_the_embedder_says() {
    let script = Script::default().plays(KEY, [Play::Say("")]);
    let mut chat = Chat::new(script, "go");

    let ending = drive(&mut chat, &mut Plain, &mut DriveState::default()).await;

    assert!(
        matches!(ending, Ending::Failed(Failure::Silent(false))),
        "{ending:?}"
    );
}

/// The stop wins the race against a model call: the step is never read, so
/// nothing joins the transcript and no step completes.
#[tokio::test]
async fn a_loop_stopped_before_it_starts_ends_stopped_and_keeps_no_step() {
    let script = Script::default().plays(KEY, [call("note", json!({"line": "never"}))]);
    let mut chat = Chat::new(script, "go");
    chat.desk.stop.cancel();
    let mut state = DriveState::default();

    let ending = drive(&mut chat, &mut Plain, &mut state).await;

    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert_eq!(ending.kind(), "stopped");
    assert_eq!(chat.transcript.messages(), [Message::User("go".to_owned())]);
    assert_eq!(lock(&chat.desk.notes).len(), 0, "the step's call never ran");
    assert_eq!(state.completed_steps(), 0);
}

/// The observer hears each call end once, with how it ended: a refusal and a
/// non-zero exit fail. A stop raised by a call skips the step's later calls
/// and ends the loop once the step is remembered.
#[tokio::test]
async fn the_observer_hears_each_call_end_and_a_stop_raised_mid_step_ends_the_loop() {
    let script = Script::default().plays(
        KEY,
        [
            Play::Call(vec![
                ("note", json!({"line": "first"})),
                ("note", json!({"exit": 3})),
                ("note", json!({"exit": 0})),
                ("missing", json!({})),
            ]),
            Play::Call(vec![
                ("note", json!({"halt": true})),
                ("note", json!({"line": "late"})),
            ]),
            Play::Say("never read"),
        ],
    );
    let mut chat = Chat::new(script, "go");
    chat.pending
        .push(Input::marked("from the inbox", Mark::new("msg-7")));
    let mut state = DriveState::default();

    let ending = drive(&mut chat, &mut Plain, &mut state).await;

    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    let mark = Some(Mark::new("msg-7"));
    let heard = |name: &str, outcome, exit_code, error_code| Heard {
        name: name.to_owned(),
        outcome,
        exit_code,
        error_code,
        mark: mark.clone(),
    };
    assert_eq!(
        chat.heard(),
        [
            heard("note", CallOutcome::Succeeded, None, None),
            heard("note", CallOutcome::Failed, Some(3), None),
            heard("note", CallOutcome::Succeeded, Some(0), None),
            heard(
                "missing",
                CallOutcome::Failed,
                None,
                Some(ErrorCode::NOT_OFFERED)
            ),
            heard("note", CallOutcome::Succeeded, None, None),
        ],
        "a call the stop reached before it began is never opened, so never heard"
    );
    assert_eq!(
        *lock(&chat.desk.notes),
        ["first"],
        "the call after the stop never ran"
    );
    assert_eq!(
        state.newest(),
        mark.as_ref(),
        "the newest mark the loop took"
    );
    assert_eq!(state.completed_steps(), 2);
    assert_eq!(
        chat.model
            .sent_by(KEY)
            .first()
            .map(|sent| sent.messages.clone()),
        Some(vec![Message::User("go\n\nfrom the inbox".to_owned())]),
        "input joins the last user message"
    );
}

/// A model call that never answers ends the run once its budget runs out;
/// the clock is tokio's paused one, so the test never waits.
#[tokio::test(start_paused = true)]
async fn a_model_that_never_answers_ends_the_run_when_its_budget_runs_out() {
    let script = Script::default().plays(KEY, [Play::Hang]);
    let mut chat = Chat::new(script, "go");
    let budget = guffbolt::Budget::Within(std::time::Duration::from_secs(30));
    let mut timed = Timed {
        budget,
        spent: false,
        pushed_back: false,
    };

    let ending = drive(&mut chat, &mut timed, &mut DriveState::default()).await;

    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert!(timed.spent, "the extension heard its budget run out");
}

#[tokio::test]
async fn a_spent_budget_ends_the_run_before_any_model_call() {
    let script = Script::default().plays(KEY, [Play::Say("never")]);
    let mut chat = Chat::new(script, "go");
    let mut timed = Timed {
        budget: guffbolt::Budget::Spent,
        spent: false,
        pushed_back: false,
    };

    let ending = drive(&mut chat, &mut timed, &mut DriveState::default()).await;

    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert_eq!(chat.model.sent_by(KEY).len(), 0);
    assert!(
        !timed.spent,
        "a budget spent before the run is not one that ran out"
    );
}

/// A window the embedder cannot fit ends the loop before the model call:
/// failed with its failure, or stopped.
#[tokio::test]
async fn a_window_that_fails_or_stops_ends_the_loop_before_the_model_call() {
    let mut failing = Chat::new(Script::default().plays(KEY, [Play::Say("never")]), "go");
    failing.fit = Some(Fitted::Failed(Failure::Exhausted));
    let mut stopping = Chat::new(Script::default().plays(KEY, [Play::Say("never")]), "go");
    stopping.fit = Some(Fitted::Stopped);

    let failed = drive(&mut failing, &mut Plain, &mut DriveState::default()).await;
    let stopped = drive(&mut stopping, &mut Plain, &mut DriveState::default()).await;

    assert!(
        matches!(failed, Ending::Failed(Failure::Exhausted)),
        "{failed:?}"
    );
    assert!(matches!(stopped, Ending::Stopped), "{stopped:?}");
    assert_eq!(
        failing.model.sent_by(KEY).len() + stopping.model.sent_by(KEY).len(),
        0
    );
}
