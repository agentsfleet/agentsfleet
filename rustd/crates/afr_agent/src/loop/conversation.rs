//! A loop's conversation between turns: what a parent sent, what the model
//! said, what the memory checkpoint wrote back, and the cap's last word.

use afd_wire::memory::MemoryDelta;
use afr_providers::{Call, Message, Replay};

use super::{EVENT_CAP_REACHED, EVENT_CHECKPOINT_FAILED, Harness, INPUT_JOIN};
use crate::context::CAP_REACHED;

impl Harness<'_, '_> {
    /// Reads what a parent sent since the last turn into the conversation,
    /// scrubbed like anything the model wrote, joined onto a user message
    /// already waiting so no provider sees two in a row.
    pub(super) fn read_input(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        while let Ok(text) = child.input.try_recv() {
            let text = self.shared.scrub.clean(text).into_inner();
            if let Some(Message::User(waiting)) = self.messages.last_mut() {
                waiting.push_str(INPUT_JOIN);
                waiting.push_str(&text);
            } else {
                self.messages.push(Message::User(text));
            }
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

    pub(super) fn cap_reached(&mut self, turns: u64, tokens: u64) {
        let lease_id = self.shared.lease_id;
        let depth = self.depth;
        let event = EVENT_CAP_REACHED;
        tracing::info!(lease_id, depth, turns, tokens, event);
        self.messages.push(Message::User(CAP_REACHED.to_owned()));
    }
}
