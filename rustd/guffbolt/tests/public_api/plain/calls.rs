//! An embedder's tools: run, refused, cut at the output limit, or held back.

use super::*;

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
