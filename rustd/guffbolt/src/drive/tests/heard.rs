//! What the scripted conversation hears of the loop: every event written down
//! in order, and the cues that bring input or pull the stop as the loop's
//! work reaches them.

use std::sync::atomic::Ordering;

use tracing::Span;

use crate::observe::{CallEnd, Observe, StepEnd, StepOutcome};
use crate::step::{Ending, Listen};
use crate::transcript::Call;

use super::fixture::Scripted;
use super::script::{Failure, lock};

impl Scripted {
    /// Writes down one thing the observer heard.
    fn note(&self, line: String) {
        lock(&self.heard).push(line);
    }
}

impl Listen for Scripted {}

impl Observe<Failure> for Scripted {
    type Call = ();

    fn step_started(&self, number: u64) -> Span {
        self.sampling.store(number, Ordering::Relaxed);
        self.ran.store(0, Ordering::Relaxed);
        // Input sent while the model streams arrives now: it waits outside
        // the conversation, so only the next call can read it.
        self.arrive(&self.during, number);
        self.note(format!("step {number} started"));
        Span::none()
    }

    fn step_ended(&self, end: StepEnd<'_, Failure>) {
        let how = match end.outcome {
            StepOutcome::Read(_step) => "read",
            StepOutcome::Failed(_failure) => "failed",
            StepOutcome::Stopped => "stopped",
        };
        self.note(format!("step {} {how}", end.number));
    }

    fn call_started(&self, call: &Call) -> ((), Span) {
        let ran = self.ran.fetch_add(1, Ordering::Relaxed);
        if ran == 0 {
            self.arrive(&self.during_calls, self.sampling.load(Ordering::Relaxed));
        }
        if self.stop_after_calls == Some(ran) {
            self.stop.cancel();
        }
        self.note(format!("call {} started", call.id));
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), end: CallEnd<'_>) {
        self.note(format!("call {} {:?}", end.call.id, end.outcome()));
        let mark = end.mark.map(|mark| mark.as_str().to_owned());
        lock(&self.ended_marks).push((end.at.step, mark));
        if let (Some(mask), Some(output)) = (self.mask, end.output) {
            mask.clone_into(&mut output.text);
        }
    }

    fn loop_ended(&self, steps: u64, ending: &Ending<Failure>) {
        self.note(format!("loop {} after {steps}", ending.kind()));
    }
}
