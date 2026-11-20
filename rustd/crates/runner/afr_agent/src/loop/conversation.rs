//! A loop's conversation between turns: what a parent sent, what the model
//! said, what the memory checkpoint wrote back; and the loop as `guffbolt`
//! drives it.

use guffbolt::{
    Call, CallRef, Fitted, Input, Mark, Request, ToolFuture, ToolSpec, Transcript, Window,
};
use tokio_util::sync::CancellationToken;

use super::{EVENT_CHECKPOINT_FAILED, Harness};
use crate::router::Lent;

impl Harness<'_, '_> {
    /// The messages a person sent the fleet since the root's last
    /// assignment, assigned to this run, each marked with its turn; a stopped
    /// loop is assigned none, and neither is an ask the run's assignment
    /// already answered.
    async fn assign_folded(&mut self) -> Vec<Input> {
        if std::mem::take(&mut self.assignment_answered) {
            return Vec::new();
        }
        let shared = self.shared;
        let mut assigned = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Vec::new(),
            assigned = shared.inbox.assign(self.folded_through) => assigned,
        };
        // An inbox that replays a message must not join it twice.
        assigned.retain(|folded| folded.sequence > self.folded_through);
        if let Some(last) = assigned.iter().map(|folded| folded.sequence).max() {
            self.folded_through = last;
        }
        assigned
            .into_iter()
            .map(|folded| {
                let mark = Mark::new(folded.turn_id.as_str());
                self.folded_inputs.push(crate::engine::FoldRef {
                    turn_id: af_wire::run::CheckpointOperationId(folded.turn_id),
                    sequence: folded.sequence,
                });
                Input::marked(folded.text, mark)
            })
            .collect()
    }

    /// The last folded message a completed model call read, once the loop
    /// answered: every message taken was read by the call that answered,
    /// because an answer is final only when no input arrived while it was
    /// written.
    pub(super) const fn folded_through(&self) -> Option<i64> {
        if self.folded_through > 0 {
            Some(self.folded_through)
        } else {
            None
        }
    }

    /// Writes the memory stored so far back, when any is; a stopped run
    /// does not wait for it. A push that fails is logged and the run goes on:
    /// the push before the report carries every entry again.
    pub(super) async fn checkpoint(&self) {
        let pending = self.shared.run.memory.pending().await;
        if pending.is_empty() {
            return;
        }
        let pushed = tokio::select! {
            biased;
            () = self.stop.cancelled() => return,
            pushed = self.shared.checkpoint.push(pending) => pushed,
        };
        if let Err(failure) = pushed {
            let error_code = failure.code().as_str();
            let run_id = self.shared.run_id;
            let event = EVENT_CHECKPOINT_FAILED;
            tracing::warn!(error_code, run_id, event);
        }
    }
}

impl guffbolt::Conversation for Harness<'_, '_> {
    type Failure = af_providers::Error;
    type Model = af_providers::Provider;
    type Observer = Self;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    /// The messages a person sent the root, each marked with its turn and
    /// scrubbed like anything the model wrote. A child's input is its task
    /// and what its parent sent it, which `guffbolt` hands it itself, so a
    /// child takes none of the root's.
    async fn take_input(&mut self) -> Vec<Input> {
        let sent = if self.depth == 0 {
            self.assign_folded().await
        } else {
            Vec::new()
        };
        let scrub = self.shared.scrub;
        sent.into_iter()
            .map(|Input { text, mark }| Input {
                text: scrub.clean(text).into_inner(),
                mark,
            })
            .collect()
    }

    /// The window `context` fits for the next call: the last compaction's
    /// notes, the groups that still fit, and the message the loop answers,
    /// which is never compacted. A fit the stop cut short ends the loop
    /// stopped; any other failure fails the run as a turn lost mid-way.
    async fn window(&mut self) -> Fitted<af_providers::Error> {
        match self.preflight().await {
            Ok(window) => Fitted::Ready(window),
            Err(_) if self.stop.is_cancelled() => Fitted::Stopped,
            Err(error) => Fitted::Failed(af_providers::Error::lost(error)),
        }
    }

    fn model(&self) -> &af_providers::Provider {
        self.shared.provider
    }

    /// The loop itself, which logs, counts and traces each turn and keeps
    /// each call in the run's ledger.
    fn observer(&self) -> &Self {
        self
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.router.specs()
    }

    /// What the window's estimate measured is what is sent.
    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        let shared = self.shared;
        Request {
            model: shared.model,
            instructions: &shared.instructions,
            messages: &window.messages,
            tools,
            hosted: &self.hosted,
            cache_key: shared.fleet_id,
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    /// Lends the call the loop and its place, which a catalog handler is
    /// lent in turn.
    fn dispatch<'a>(&'a self, call: &'a Call, call_ref: &'a CallRef) -> ToolFuture<'a> {
        let lent = Lent {
            harness: self,
            call_ref,
        };
        self.router.dispatch(&call.name, &call.arguments, lent)
    }

    /// What the model said and called, as the transcript keeps it: scrubbed,
    /// so no secret value is ever sent to the model, whoever wrote it. The
    /// router ran each call with the arguments as the model wrote them. The
    /// provider's replay goes back unopened: it is the provider's own record
    /// of its reasoning, signed where the provider signs it.
    fn scrub(&self, text: &mut String, calls: &mut [Call]) {
        let scrub = self.shared.scrub;
        *text = scrub.clean(std::mem::take(text)).into_inner();
        for call in calls {
            call.arguments = scrub
                .clean_json(std::mem::take(&mut call.arguments))
                .into_inner();
        }
    }

    /// The run's token for the root, its own for a child.
    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, cut: bool) -> af_providers::Error {
        af_providers::Error::silent(cut)
    }

    async fn called(&mut self) {
        if self.checkpoints.due() {
            self.checkpoint().await;
        }
    }
}
