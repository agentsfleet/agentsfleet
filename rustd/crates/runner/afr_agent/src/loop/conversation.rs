//! A loop's conversation between turns: what a parent sent, what the model
//! said, what the memory checkpoint wrote back, and the cap's last word; and
//! the loop as `guffbolt` drives it.

use af_providers::{Call, Message, Replay};
use af_wire::memory::MemoryDelta;
use guffbolt::{Prepared, Sampled};

use super::{EVENT_CHECKPOINT_FAILED, Harness, INPUT_JOIN};
use crate::nested::Tether;
use crate::turn::Turn;

impl Harness<'_, '_> {
    /// What a parent sent a child since the child's last turn.
    fn sent_by_parent(tether: &mut Tether<'_, '_>) -> Vec<String> {
        let mut sent = Vec::new();
        while let Ok(text) = tether.input.try_recv() {
            sent.push(text);
        }
        sent
    }

    /// The messages a person sent the fleet since the root's last claim,
    /// claimed for this run; a stopped loop claims none, and neither does an
    /// ask the lease's claim already answered.
    async fn claim_folded(&mut self) -> Vec<String> {
        if std::mem::take(&mut self.claim_answered) {
            return Vec::new();
        }
        let shared = self.shared;
        let mut claimed = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Vec::new(),
            claimed = shared.inbox.claim(self.folded_through) => claimed,
        };
        // An inbox that replays a message must not join it twice.
        claimed.retain(|folded| folded.sequence > self.folded_through);
        if let Some(last) = claimed.iter().map(|folded| folded.sequence).max() {
            self.folded_through = last;
        }
        claimed.into_iter().map(|folded| folded.text).collect()
    }

    /// Adds `text` to the conversation, scrubbed like anything the model
    /// wrote, joined onto a user message already waiting so no provider sees
    /// two in a row.
    fn join_user(&mut self, text: String) {
        let text = self.shared.scrub.clean(text).into_inner();
        if let Some(Message::User(waiting)) = self.messages.last_mut() {
            waiting.push_str(INPUT_JOIN);
            waiting.push_str(&text);
        } else {
            self.messages.push(Message::User(text));
        }
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

    /// Writes the memory stored so far back, when any is; a stopped lease
    /// does not wait for it. A push that fails is logged and the run goes on:
    /// the push before the report carries every entry again.
    pub(super) async fn checkpoint(&self) {
        let pending: Vec<MemoryDelta<'static>> = (self.shared.lease.memory.lock().await)
            .pending()
            .into_iter()
            .map(MemoryDelta::into_owned)
            .collect();
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
            let lease_id = self.shared.lease_id;
            let event = EVENT_CHECKPOINT_FAILED;
            tracing::warn!(error_code, lease_id, event);
        }
    }

    /// What the model said and called, as the conversation keeps it: scrubbed,
    /// so no secret value is ever sent to the model, whoever wrote it. The
    /// router ran each call with the arguments as the model wrote them. The
    /// provider's replay goes back unopened: it is the provider's own record
    /// of its reasoning, signed where the provider signs it.
    pub(super) fn remembered(&self, text: String, calls: Vec<Call>, replay: Replay) -> Message {
        let scrub = self.shared.scrub;
        let calls = calls
            .into_iter()
            .map(|call| Call {
                arguments: scrub.clean_json(call.arguments).into_inner(),
                ..call
            })
            .collect();
        Message::Assistant {
            text: scrub.clean(text).into_inner(),
            calls,
            replay,
        }
    }
}

impl guffbolt::Conversation for Harness<'_, '_> {
    type Turn = Turn;
    type Results = Vec<Message>;
    type Failure = af_providers::Error;

    async fn take_input(&mut self) -> bool {
        let sent = match self.child.as_mut() {
            Some(tether) => Self::sent_by_parent(tether),
            None => self.claim_folded().await,
        };
        let took = !sent.is_empty();
        for text in sent {
            self.join_user(text);
        }
        took
    }

    async fn prepare(&mut self) -> Prepared<af_providers::Error> {
        match self.preflight().await {
            Ok(()) => Prepared::Ready,
            Err(_) if self.stop.is_cancelled() => Prepared::Stopped,
            Err(error) => Prepared::Failed(af_providers::Error::lost(error)),
        }
    }

    fn begin_group(&mut self) {
        self.group_start = self.messages.len();
    }

    async fn sample(&mut self, number: u64) -> Sampled<Turn, af_providers::Error> {
        match self.turn(number, false).await {
            Some(Ok(turn)) => Sampled::Turn(turn),
            Some(Err(failure)) => Sampled::Failed(failure),
            None => Sampled::Stopped,
        }
    }

    async fn run_calls(&mut self, turn: &Turn) -> Vec<Message> {
        let mut results = Vec::with_capacity(turn.calls.len());
        for call in &turn.calls {
            match self.call(call, turn.cut).await {
                Some(result) => results.push(result),
                None => break,
            }
            if self.checkpoints.due() {
                self.checkpoint().await;
            }
        }
        results
    }

    fn remember(&mut self, turn: Turn, results: Vec<Message>) {
        let said = self.remembered(turn.text, turn.calls, turn.replay);
        self.messages.push(said);
        self.messages.extend(results);
    }

    fn push_user(&mut self, text: String) {
        self.messages.push(Message::User(text));
    }

    fn stopped(&self) -> bool {
        self.stop.is_cancelled()
    }

    fn silent(&self, cut: bool) -> af_providers::Error {
        af_providers::Error::silent(cut)
    }
}
