//! Where each call sits: the loop hands every dispatch its step and its
//! place in that step, a retry inside a tool keeps that reference, and a
//! resumed loop goes on from the steps it completed.

use std::future::ready;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::step::Ending;
use crate::tool::{Describe, ErrorCode, Tool, ToolFuture, ToolOutput, ToolSpec};
use crate::transcript::{CallRef, Message};
use crate::{DriveState, Plain};

use super::drive;
use super::fixture::{Lent, Scripted, answered};
use super::script::{TOOL, answer, asking, calling, lock, naming};

/// The tool whose backend loses its first reply.
const PUBLISH: &str = "publish";

/// The reference of call `index` of step `step`, whose provider id is
/// `call_id`.
fn at(step: u64, index: u64, call_id: &str) -> CallRef {
    CallRef {
        step,
        index,
        call_id: call_id.to_owned(),
    }
}

/// The reply a timed-out attempt never got.
#[derive(Debug)]
struct Lost;

/// A remote ledger whose first reply is lost: the effect commits, then the
/// caller hears nothing back. It keeps one effect per reference, so a retry
/// it has seen adds none.
#[derive(Debug, Default)]
struct Ledger {
    /// The reference each attempt carried, in order.
    attempts: Mutex<Vec<CallRef>>,
    /// The effects it committed.
    effects: Mutex<Vec<CallRef>>,
}

impl Ledger {
    /// Commits the effect `call_ref` names, once; the first attempt's reply
    /// is lost.
    fn publish(&self, call_ref: &CallRef) -> Result<(), Lost> {
        let mut effects = lock(&self.effects);
        if !effects.contains(call_ref) {
            effects.push(call_ref.clone());
        }
        let mut attempts = lock(&self.attempts);
        attempts.push(call_ref.clone());
        if attempts.len() == 1 {
            Err(Lost)
        } else {
            Ok(())
        }
    }
}

/// `publish`: sends to the ledger, and tries once more when the reply is
/// lost, with the reference its call was lent.
#[derive(Debug)]
struct Publish {
    ledger: Arc<Ledger>,
    parameters: Value,
}

impl Describe for Publish {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: PUBLISH,
            description: "Publishes the release notes.",
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Lent<'c>> for Publish {
    fn call<'a>(&'a self, _arguments: &'a Value, cx: Lent<'c>) -> ToolFuture<'a>
    where
        Lent<'c>: 'a,
    {
        let sent = self
            .ledger
            .publish(cx)
            .or_else(|Lost| self.ledger.publish(cx));
        Box::pin(ready(match sent {
            Ok(()) => ToolOutput::succeeded("published"),
            Err(Lost) => ToolOutput::failed(ErrorCode::INTERRUPTED, "the ledger never answered"),
        }))
    }
}

#[tokio::test]
async fn a_retried_call_keeps_its_call_ref() {
    let ledger = Arc::new(Ledger::default());
    let publish = Publish {
        ledger: Arc::clone(&ledger),
        parameters: json!({ "type": "object" }),
    };
    let mut run = Scripted::new([
        naming([(TOOL, Value::Null), (PUBLISH, Value::Null)]),
        answer("Published."),
    ])
    .offering(Box::new(publish));

    let ending = drive(&mut run, &mut Plain, &mut DriveState::default()).await;

    let publishing = at(1, 1, "call-1-1");
    assert_eq!(
        *lock(&ledger.attempts),
        [publishing.clone(), publishing.clone()],
        "the lost attempt and its retry carry the same reference"
    );
    assert_eq!(
        *lock(&ledger.effects),
        [publishing],
        "the retry adds no second effect"
    );
    assert_eq!(answered(&ending), Some("Published."));
    let result = Message::ToolResult {
        call_id: "call-1-1".to_owned(),
        output: "published".to_owned(),
        image: None,
    };
    assert!(
        run.messages().contains(&result),
        "the model reads the retry's success: {:?}",
        run.messages()
    );
}

#[tokio::test]
async fn each_call_ref_names_its_step_and_its_place_in_that_step() {
    let mut run = Scripted::new([calling(2), calling(1), answer("Done.")]);

    let ending = drive(&mut run, &mut Plain, &mut DriveState::default()).await;

    assert_eq!(answered(&ending), Some("Done."));
    assert_eq!(
        run.dispatched(),
        [
            at(1, 0, "call-1-0"),
            at(1, 1, "call-1-1"),
            at(2, 0, "call-2-0"),
        ],
        "one step's calls share its number and differ by index; the next step counts one more"
    );
}

#[tokio::test]
async fn a_resumed_loop_numbers_its_steps_after_the_saved_ones() {
    let mut run = Scripted::new([
        naming([(TOOL, Value::Null), asking("Which region?")]),
        calling(1),
        answer("Done."),
    ]);
    let mut state = DriveState::default();

    let first = drive(&mut run, &mut Plain, &mut state).await;
    state.resume(&mut run.transcript, "eu-west");
    let ending = drive(&mut run, &mut Plain, &mut state).await;

    assert!(matches!(first, Ending::Waiting(_)), "{first:?}");
    assert_eq!(answered(&ending), Some("Done."));
    assert_eq!(
        run.dispatched(),
        [
            at(1, 0, "call-1-0"),
            at(1, 1, "call-1-1"),
            at(2, 0, "call-2-0"),
        ],
        "the resumed loop's first call is step 2, never step 1 again"
    );
}
