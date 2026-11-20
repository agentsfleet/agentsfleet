//! Questions that wait, answers the embedder rewrites or sends back, and the
//! replay a provider keeps.

use super::*;

/// A call that asks the person ends the loop waiting; the answer, given
/// through the saved state, is the asking call's result on the next drive.
#[tokio::test]
async fn a_question_waits_and_the_answer_resumes_the_loop() {
    let script = Script::default().plays(
        KEY,
        [
            Play::Call(vec![
                ("note", json!({"ask": "which shop?"})),
                ("note", json!({"line": "bread"})),
            ]),
            Play::Say("going to the corner shop"),
        ],
    );
    let mut chat = Chat::new(script, "shop");
    let mut state = DriveState::default();

    let first = drive(&mut chat, &mut Plain, &mut state).await;
    let waiting = match first {
        Ending::Waiting(waiting) => Some(waiting),
        Ending::Answered(_) | Ending::Failed(_) | Ending::Stopped => None,
    }
    .expect("the loop waits on the question");
    assert_eq!(waiting.question, "which shop?");
    assert_eq!(
        *lock(&chat.desk.notes),
        ["bread"],
        "the step's other call ran"
    );
    let saved = serde_json::to_string(&state).expect("a waiting state encodes");
    let mut state: DriveState = serde_json::from_str(&saved).expect("and decodes");
    assert_eq!(state.waiting(), Some(&waiting));

    let resumed = state.resume(chat.transcript(), "the corner shop");
    let second = drive(&mut chat, &mut Plain, &mut state).await;

    assert_eq!(resumed, Some(waiting));
    assert_eq!(answered(&second), Some("going to the corner shop"));
    assert_eq!(state.waiting(), None);
    assert!(
        results(chat.transcript.messages()).contains(&"the corner shop".to_owned()),
        "the answer is the asking call's result"
    );
    assert_eq!(
        state.resume(chat.transcript(), "again"),
        None,
        "nothing waits any more"
    );
}

/// A state that already waits on a question ends waiting at once, calling
/// no model, until the question is answered.
#[tokio::test]
async fn a_state_waiting_on_a_question_ends_waiting_without_a_model_call() {
    let script = Script::default().plays(
        KEY,
        [
            call("note", json!({"ask": "which colour?"})),
            Play::Say("blue it is"),
        ],
    );
    let mut chat = Chat::new(script, "paint");
    let mut state = DriveState::default();
    let first = drive(&mut chat, &mut Plain, &mut state).await;
    assert_eq!(first.kind(), "waiting");

    let again = drive(&mut chat, &mut Plain, &mut state).await;

    assert!(
        matches!(&again, Ending::Waiting(waiting) if waiting.question == "which colour?"),
        "{again:?}"
    );
    assert_eq!(chat.model.sent_by(KEY).len(), 1, "no second model call");
}

/// An embedder rewrites what a resumed run carries over through the state.
#[test]
fn map_answers_rewrites_every_accepted_answer() {
    let mut state: DriveState =
        serde_json::from_value(json!({"answers": ["key sk-123", "fine"], "completed_steps": 2}))
            .expect("a saved state");

    state.map_answers(|answer| answer.replace("sk-123", "[masked]"));

    assert_eq!(state.answers(), ["key [masked]", "fine"]);
    assert_eq!(state.completed_steps(), 2, "only the answers change");
}

/// An answer the extension sends back to work runs another step, and the
/// run ends with every accepted answer.
#[tokio::test]
async fn an_answer_sent_back_to_work_runs_another_step() {
    let script = Script::default().plays(KEY, [Play::Say("halfway"), Play::Say("all done")]);
    let mut chat = Chat::new(script, "go");
    let mut timed = Timed {
        budget: guffbolt::Budget::Unbounded,
        spent: false,
        pushed_back: false,
    };
    let mut state = DriveState::default();

    let ending = drive(&mut chat, &mut timed, &mut state).await;

    assert_eq!(answered(&ending), Some("all done"));
    let second = chat
        .model
        .sent_by(KEY)
        .get(1)
        .map(|sent| sent.messages.clone());
    assert_eq!(
        second.and_then(|messages| messages.last().cloned()),
        Some(Message::User("keep going until it is done".to_owned())),
        "the push back reaches the model as input"
    );
}

/// What a provider records to continue its reasoning is kept as written, and
/// serializes as the record itself.
#[test]
fn a_replay_keeps_the_providers_record_as_written() {
    let replay = guffbolt::Replay::new(json!({"encrypted": "r1"}));
    let none = guffbolt::Replay::default();

    let encoded = serde_json::to_value(&replay).expect("encodes");

    assert_eq!(replay.record(), &json!({"encrypted": "r1"}));
    assert!(!replay.is_empty());
    assert!(none.is_empty());
    assert_eq!(encoded, json!({"encrypted": "r1"}));
    assert_eq!(guffbolt::Hosted::WebSearch.name(), "web_search");
}
