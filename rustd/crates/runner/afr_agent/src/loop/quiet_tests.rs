#![expect(
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! A fleet run that asks the person while a child it started still runs
//! ends waiting only once that child has answered: `guffbolt`'s root waits
//! for the run's children itself, and `Harness` adds nothing for it.

use afr_tools::catalog::MESSAGE;
use afr_tools::{CatalogTool, Entry, ToolContext};
use guffbolt::agents::SPAWN_AGENT;
use guffbolt::tools::{NoArguments, Schema};
use guffbolt::{Describe, Message, Tool, ToolFuture, ToolOutput, ToolSpec};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::finish::DETAIL_WAITING;
use super::tests::{drive, engine};
use crate::ResultOutcome;
use crate::fixture::{Script, call, payload, say, unbounded};

/// The task the root hands a child, which opens the child's conversation.
const DELEGATED: &str = "read the build log";

/// A tool whose every call asks the person a question. The fleet catalog
/// publishes no question tool, so it stands under another entry's name.
#[derive(Debug)]
struct Asks {
    entry: &'static Entry,
    schema: Schema,
}

impl Asks {
    fn boxed(entry: &'static Entry) -> Box<CatalogTool> {
        Box::new(Self {
            entry,
            schema: Schema::of::<NoArguments>(entry.name()),
        })
    }
}

impl Describe for Asks {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(self.entry.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Asks {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        _cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(std::future::ready(ToolOutput::asks("Ship it?")))
    }
}

#[tokio::test]
async fn test_a_question_waits_for_a_running_child_to_answer() {
    let script = Script::new([vec![
        call(
            "p1",
            SPAWN_AGENT,
            json!({"message": DELEGATED, "fork_turns": "none"}),
        ),
        call("p2", MESSAGE.name(), json!({})),
    ]])
    .with_child(DELEGATED, [vec![say("read")]]);
    let engine = engine(vec![Asks::boxed(&MESSAGE)], &script);
    let run = payload(&[SPAWN_AGENT, MESSAGE.name()], unbounded());

    let (output, _frames) = drive(&engine, &run, &CancellationToken::new()).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a run waiting on a question is not a completed one");
    };
    assert_eq!(failure.detail, DETAIL_WAITING);
    let sent = script.sent();
    let child = sent.iter().filter(|request| {
        matches!(request.messages.first(), Some(Message::User(text)) if text.starts_with(DELEGATED))
    });
    assert_eq!(
        (sent.len(), child.count()),
        (2, 1),
        "the child ran its turn before the run ended waiting"
    );
}
