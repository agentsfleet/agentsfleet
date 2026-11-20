//! The loop with no generic tools: an embedder's own tool, lent the
//! embedder's own state, and every way a call or a step is refused.

use std::future::ready;
use std::sync::Mutex;

use guffbolt::{
    Call, CallEnd, CallOutcome, CallRef, Conversation, Describe, DriveState, Ending, ErrorCode,
    Fitted, Input, Listen, Mark, Message, Observe, Plain, Request, Tool, ToolFuture, ToolOutput,
    ToolSpec, Tools, Transcript, Window, drive,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tracing::Span;

use crate::script::{Failure, Play, Script, call, lock, results};

/// The key every request of this file's loop carries.
const KEY: &str = "plain";

/// What every call is lent: the lines `note` wrote, and the loop's stop.
#[derive(Debug, Default)]
struct Desk {
    notes: Mutex<Vec<String>>,
    stop: CancellationToken,
}

/// Writes down the line it is given; refuses a blank one; asks the person,
/// stops the loop or answers with a process's exit code when told to.
#[derive(Debug)]
struct Note {
    parameters: Value,
}

impl Describe for Note {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "note",
            description: "Writes a line down.",
            parameters: &self.parameters,
        }
    }
}

impl<'n> Tool<&'n Desk> for Note {
    fn call<'a>(&'a self, arguments: &'a Value, cx: &'n Desk) -> ToolFuture<'a>
    where
        &'n Desk: 'a,
    {
        Box::pin(async move {
            if let Some(question) = arguments["ask"].as_str() {
                return ToolOutput::asks(question);
            }
            if arguments["halt"].as_bool() == Some(true) {
                cx.stop.cancel();
                return ToolOutput::succeeded("halted");
            }
            if let Some(code) = arguments["exit"].as_i64() {
                let mut exited = ToolOutput::succeeded(format!("exited {code}"));
                exited.exit_code = i32::try_from(code).ok();
                return exited;
            }
            match arguments["line"].as_str() {
                Some(line) if !line.is_empty() => {
                    lock(&cx.notes).push(line.to_owned());
                    ToolOutput::succeeded(format!("noted {line}"))
                }
                _ => ToolOutput::failed(ErrorCode::new("blank_line"), "a note needs a line"),
            }
        })
    }
}

/// One call's end as the observer heard it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Heard {
    name: String,
    outcome: CallOutcome,
    exit_code: Option<i32>,
    error_code: Option<ErrorCode>,
    mark: Option<Mark>,
}

/// Hears each call's end.
#[derive(Debug, Default)]
struct Watch {
    heard: Mutex<Vec<Heard>>,
}

impl Listen for Watch {}

impl Observe<Failure> for Watch {
    type Call = ();

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), end: CallEnd<'_>) {
        let heard = Heard {
            name: end.call.name.clone(),
            outcome: end.outcome(),
            exit_code: end.exit_code(),
            error_code: end.error_code(),
            mark: end.mark.cloned(),
        };
        lock(&self.heard).push(heard);
    }
}

/// A chat over the scripted model, holding `note`.
struct Chat {
    model: Script,
    tools: Tools<dyn for<'n> Tool<&'n Desk>>,
    desk: Desk,
    watch: Watch,
    pending: Vec<Input>,
    /// What the next window fitting says, once; the whole transcript after.
    fit: Option<Fitted<Failure>>,
    transcript: Transcript,
}

impl Chat {
    fn new(model: Script, opening: &str) -> Self {
        let note: Box<dyn for<'n> Tool<&'n Desk>> = Box::new(Note {
            parameters: json!({"type": "object"}),
        });
        Self {
            model,
            tools: Tools::new(vec![note]),
            desk: Desk::default(),
            watch: Watch::default(),
            pending: Vec::new(),
            fit: None,
            transcript: Transcript::new(vec![Message::User(opening.to_owned())]),
        }
    }

    fn heard(&self) -> Vec<Heard> {
        lock(&self.watch.heard).clone()
    }
}

impl Conversation for Chat {
    type Failure = Failure;
    type Model = Script;
    type Observer = Watch;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(std::mem::take(&mut self.pending))
    }

    fn window(&mut self) -> impl Future<Output = Fitted<Failure>> + Send {
        let fitted = self
            .fit
            .take()
            .unwrap_or_else(|| Fitted::Ready(Window::whole(self.transcript.messages())));
        ready(fitted)
    }

    fn model(&self) -> &Script {
        &self.model
    }

    fn observer(&self) -> &Watch {
        &self.watch
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.tools.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "script",
            instructions: "",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: KEY,
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, _call_ref: &'a CallRef) -> ToolFuture<'a> {
        self.tools.dispatch(&call.name, &call.arguments, &self.desk)
    }

    fn stop(&self) -> &CancellationToken {
        &self.desk.stop
    }

    fn silent(&self, cut: bool) -> Failure {
        Failure::Silent(cut)
    }
}

/// The text of an answered ending.
fn answered(ending: &Ending<Failure>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

#[tokio::test]
async fn an_embedders_tool_runs_and_the_loop_answers() {
    let script = Script::default().plays(
        KEY,
        [
            call("note", json!({"line": "milk"})),
            Play::Say("noted milk"),
        ],
    );
    let mut chat = Chat::new(script, "remember milk");
    let mut state = DriveState::default();

    let ending = drive(&mut chat, &mut Plain, &mut state).await;

    assert_eq!(answered(&ending), Some("noted milk"));
    assert_eq!(*lock(&chat.desk.notes), ["milk"]);
    assert_eq!(results(chat.transcript.messages()), ["noted milk"]);
    assert_eq!(state.completed_steps(), 2, "one tool step, one answer");
    assert_eq!(state.answers(), ["noted milk"]);
    assert_eq!(
        chat.model
            .sent_by(KEY)
            .first()
            .map(|sent| sent.tools.clone()),
        Some(vec!["note".to_owned()]),
        "the model is offered the embedder's tool"
    );
}

/// A name the loop holds no tool for, and a call its own tool refuses, both
/// answer the model, and the loop goes on to the next step.
#[tokio::test]
async fn a_refused_call_answers_the_model_and_the_loop_continues() {
    let script = Script::default().plays(
        KEY,
        [
            Play::Call(vec![
                ("teleport", json!({"to": "mars"})),
                ("note", json!({"line": ""})),
                ("note", json!({"line": "eggs"})),
            ]),
            Play::Say("noted what I could"),
        ],
    );
    let mut chat = Chat::new(script, "go");

    let ending = drive(&mut chat, &mut Plain, &mut DriveState::default()).await;

    assert_eq!(answered(&ending), Some("noted what I could"));
    assert_eq!(
        results(chat.transcript.messages()),
        [
            "[tool_not_offered] teleport is not one of this run's tools",
            "[blank_line] a note needs a line",
            "noted eggs",
        ]
    );
    assert_eq!(
        *lock(&chat.desk.notes),
        ["eggs"],
        "only the call that parsed ran"
    );
    let second = chat.model.sent_by(KEY);
    assert_eq!(
        second.len(),
        2,
        "the model was called again after the refusals"
    );
}

/// A step cut at the output limit runs none of its calls: each reads the
/// harness's refusal instead.
#[tokio::test]
async fn a_step_cut_at_the_output_limit_runs_none_of_its_calls() {
    let script = Script::default().plays(
        KEY,
        [
            Play::Cut(vec![("note", json!({"line": "half"}))]),
            Play::Say("stopped short"),
        ],
    );
    let mut chat = Chat::new(script, "go");

    let ending = drive(&mut chat, &mut Plain, &mut DriveState::default()).await;

    assert_eq!(answered(&ending), Some("stopped short"));
    assert_eq!(lock(&chat.desk.notes).len(), 0, "the cut call never ran");
    let refusals = results(chat.transcript.messages());
    assert_eq!(refusals.len(), 1);
    assert!(
        refusals
            .first()
            .is_some_and(|refusal| refusal.starts_with("[output_limit_reached] note ")),
        "{refusals:?}"
    );
}

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

/// A task bounded in time: its budget, its record of a spent budget, and
/// an answer it sends back to work once.
struct Timed {
    budget: guffbolt::Budget,
    spent: bool,
    pushed_back: bool,
}

impl guffbolt::Extension<Chat> for Timed {
    fn budget(&self, _chat: &Chat) -> guffbolt::Budget {
        self.budget
    }

    fn budget_spent(&mut self, _chat: &mut Chat) {
        self.spent = true;
    }

    fn on_answer(
        &mut self,
        _chat: &mut Chat,
        answer: &str,
    ) -> impl Future<Output = guffbolt::Answer<Failure>> + Send {
        let decided = if self.pushed_back || answer.contains("done") {
            guffbolt::Answer::Accept
        } else {
            self.pushed_back = true;
            guffbolt::Answer::Continue("keep going until it is done".to_owned())
        };
        ready(decided)
    }
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

/// A tool held unoffered is never named to the model, yet a call to it
/// still reaches it; the set reads back what it offers and holds.
#[tokio::test]
async fn a_held_tool_is_not_offered_but_still_answers_a_call() {
    let note = |name| -> Box<dyn for<'n> Tool<&'n Desk>> {
        Box::new(Named {
            name,
            parameters: json!({"type": "object"}),
        })
    };
    let mut tools: Tools<dyn for<'n> Tool<&'n Desk>> = Tools::new(vec![note("seen")]);
    tools.offer(note("also_seen"));
    tools.hold(note("hidden"));
    let desk = Desk::default();

    let held = tools.dispatch("hidden", &json!({}), &desk).await;
    let offered: Vec<String> = tools.specs().map(|spec| spec.name.to_owned()).collect();
    let unoffered: Vec<String> = tools
        .held_specs()
        .map(|spec| spec.name.to_owned())
        .collect();
    let debug = format!("{tools:?}");
    let boxed = tools.into_offered();
    let first = boxed.first().expect("one offered");
    let through_the_box = first.call(&json!({}), &desk).await;

    assert_eq!(held.text, "hidden answered");
    assert_eq!(offered, ["seen", "also_seen"]);
    assert_eq!(unoffered, ["hidden"]);
    assert_eq!(
        debug,
        r#"Tools { offered: ["seen", "also_seen"], unoffered: ["hidden"] }"#
    );
    assert_eq!(
        (first.name(), through_the_box.text.as_str()),
        ("seen", "seen answered")
    );
}

/// A tool that answers with its own name.
#[derive(Debug)]
struct Named {
    name: &'static str,
    parameters: Value,
}

impl Describe for Named {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: self.name,
            description: "Answers.",
            parameters: &self.parameters,
        }
    }
}

impl<'n> Tool<&'n Desk> for Named {
    fn call<'a>(&'a self, _arguments: &'a Value, _cx: &'n Desk) -> ToolFuture<'a>
    where
        &'n Desk: 'a,
    {
        Box::pin(ready(ToolOutput::succeeded(format!(
            "{} answered",
            self.name
        ))))
    }
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
