//! A queue inside this process: one channel, one worker, a fixed retry ladder.
//!
//! The channel is bounded, so a burst cannot grow memory without limit; when it
//! is full the offer is refused and the producer does the work itself. Messages
//! leave in the order they arrived, which a consumer may rely on for messages
//! offered by one producer. Nothing survives the process: a domain that needs
//! that keeps its own durable record and its own repair, and the message is
//! only the fast path.

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, error::TrySendError};

use super::{Consumer, Offer, Outcome, Sink};

/// How a [`Memory`] queue is sized and how hard it tries.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Messages each worker holds waiting before an offer to it is refused.
    pub depth: usize,
    /// Workers, each consuming its own shard in order.
    pub workers: usize,
    /// Pauses between attempts after a [`Outcome::Retry`]; one more attempt
    /// than pauses, then [`Consumer::abandoned`].
    pub retry: &'static [Duration],
}

/// Picks the shard a message belongs to; messages with one key stay in order.
pub type Shard<M> = Arc<dyn Fn(&M) -> u64 + Send + Sync>;

/// A [`Sink`] whose messages worker tasks in this process hand to a [`Consumer`].
pub struct Memory<M> {
    shards: Vec<mpsc::Sender<M>>,
    shard: Shard<M>,
}

impl<M> Debug for Memory<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory")
            .field("workers", &self.shards.len())
            .finish_non_exhaustive()
    }
}

impl<M: Send + Sync + 'static> Memory<M> {
    /// Starts `settings.workers` workers on the current Tokio runtime, routing
    /// each message by `shard`. Outside a runtime there are no workers, and
    /// every offer is refused so the producer does the work.
    #[must_use]
    pub fn start(consumer: &Arc<dyn Consumer<M>>, settings: Settings, shard: Shard<M>) -> Self {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Self {
                shards: Vec::new(),
                shard,
            };
        };
        let shards = (0..settings.workers.max(1))
            .map(|_| {
                let (messages, queue) = mpsc::channel(settings.depth.max(1));
                runtime.spawn(work(Arc::clone(consumer), queue, settings.retry));
                messages
            })
            .collect();
        Self { shards, shard }
    }
}

impl<M: Send + Sync + 'static> Sink<M> for Memory<M> {
    fn offer(&self, message: M) -> Offer<M> {
        let count = self.shards.len() as u64;
        let Some(messages) = (count > 0)
            .then(|| (self.shard)(&message) % count)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.shards.get(index))
        else {
            return Offer::Refused(message);
        };
        match messages.try_send(message) {
            Ok(()) => Offer::Accepted,
            Err(TrySendError::Full(message) | TrySendError::Closed(message)) => {
                tracing::debug!(
                    event = EVENT_REFUSED,
                    "the queue is full; the producer does the work"
                );
                Offer::Refused(message)
            }
        }
    }
}

const EVENT_REFUSED: &str = "queue_offer_refused";

async fn work<M: Send + Sync + 'static>(
    consumer: Arc<dyn Consumer<M>>,
    mut queue: mpsc::Receiver<M>,
    retry: &'static [Duration],
) {
    while let Some(message) = queue.recv().await {
        deliver(consumer.as_ref(), &message, retry).await;
    }
}

/// Hands `message` to `consumer` until it is done or the ladder runs out.
async fn deliver<M: Send + Sync + 'static>(
    consumer: &dyn Consumer<M>,
    message: &M,
    retry: &'static [Duration],
) {
    let mut pauses = retry.iter();
    while consumer.consume(message).await == Outcome::Retry {
        let Some(pause) = pauses.next() else {
            consumer.abandoned(message);
            return;
        };
        tokio::time::sleep(*pause).await;
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
