//! What the fold suites share: inboxes that deliver on cue, the run they
//! drive, and the requests the scripted model was sent.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use af_egress::testing::CountingMint;
use af_providers::Message;
use af_wire::lease::LeasePayload;
use afr_tools::catalog::HTTP_REQUEST;
use tokio_util::sync::CancellationToken;

use crate::engine::{AgentEngine as _, AgentRun, Meter, RunOutput};
use crate::fixture::{Canned, Script};
use crate::harness::tests::engine;
use crate::testing::{ContextMemory, Discard};
use crate::{Folded, Inbox};

/// Locks `mutex`; a test that panicked holding it leaves its data readable.
pub(super) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An inbox whose messages arrive on cue: each is claimable from the claim
/// whose index (from 0) it names on, the way a person's send lands between
/// two model calls.
#[derive(Debug, Default)]
pub(super) struct Arriving {
    scheduled: Mutex<Vec<(usize, Folded)>>,
    claims: AtomicUsize,
    asked: Mutex<Vec<i64>>,
}

impl Arriving {
    pub(super) fn on_claim(&self, claim: usize, sequence: i64, text: &str) {
        locked(&self.scheduled).push((
            claim,
            Folded {
                sequence,
                text: text.to_owned(),
            },
        ));
    }

    pub(super) fn asked(&self) -> Vec<i64> {
        locked(&self.asked).clone()
    }
}

#[async_trait::async_trait]
impl Inbox for Arriving {
    async fn claim(&self, after_sequence: i64) -> Vec<Folded> {
        let claim = self.claims.fetch_add(1, Ordering::Relaxed);
        locked(&self.asked).push(after_sequence);
        let mut ready: Vec<Folded> = locked(&self.scheduled)
            .iter()
            .filter(|(on, folded)| *on <= claim && folded.sequence > after_sequence)
            .map(|(_, folded)| folded.clone())
            .collect();
        ready.sort_by_key(|folded| folded.sequence);
        ready
    }
}

/// An inbox that answers the same message to its first claims, whatever the
/// cursor they ask from: a daemon that lost track of what it had handed out.
#[derive(Debug)]
pub(super) struct Replaying {
    pub(super) message: Folded,
    pub(super) replays: usize,
    pub(super) claims: AtomicUsize,
}

#[async_trait::async_trait]
impl Inbox for Replaying {
    async fn claim(&self, _after_sequence: i64) -> Vec<Folded> {
        if self.claims.fetch_add(1, Ordering::Relaxed) < self.replays {
            vec![self.message.clone()]
        } else {
            Vec::new()
        }
    }
}

/// An inbox whose claim is still being answered when the lease stops.
#[derive(Debug)]
pub(super) struct Interrupted {
    pub(super) stop: CancellationToken,
}

#[async_trait::async_trait]
impl Inbox for Interrupted {
    async fn claim(&self, _after_sequence: i64) -> Vec<Folded> {
        self.stop.cancel();
        tokio::task::yield_now().await;
        vec![Folded {
            sequence: 3,
            text: "too late".to_owned(),
        }]
    }
}

pub(super) async fn run_with(
    script: &Script,
    payload: &LeasePayload<'_>,
    inbox: &dyn Inbox,
    stop: &CancellationToken,
) -> crate::Result<RunOutput> {
    let engine = engine(vec![Canned::boxed(&HTTP_REQUEST, "deployed")], script);
    engine
        .run(AgentRun {
            recovered_task: None,
            prepared_task: None,
            lease: payload,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            context_checkpoints: &ContextMemory::default(),
            inbox,
            events: &|_frame| {},
            meter: &Meter::default(),
            stop,
        })
        .await
}

/// A provider that hangs up mid-turn.
pub(super) fn lost() -> af_providers::Error {
    af_providers::Error::lost("the provider hung up")
}

/// The user messages request `index` carried, in order.
pub(super) fn users(script: &Script, index: usize) -> Vec<String> {
    script.sent().get(index).map_or_else(Vec::new, |sent| {
        sent.messages
            .iter()
            .filter_map(|message| match message {
                Message::User(text) => Some(text.clone()),
                Message::Assistant { .. } | Message::ToolResult { .. } => None,
            })
            .collect()
    })
}

/// Each message of request `index`, as its role and text, in order.
pub(super) fn shape(script: &Script, index: usize) -> Vec<(&'static str, String)> {
    script.sent().get(index).map_or_else(Vec::new, |sent| {
        sent.messages
            .iter()
            .map(|message| match message {
                Message::User(text) => ("user", text.clone()),
                Message::Assistant { text, .. } => ("assistant", text.clone()),
                Message::ToolResult { output, .. } => ("tool", output.clone()),
            })
            .collect()
    })
}
