//! Work moved off the request path, behind one seam.
//!
//! ```text
//!   producer ──offer(M)──► Sink<M> ──(a realization carries M)──► Consumer<M>::consume
//!      ▲                      │
//!      └── Offer::Refused(M) ─┘   full, closed or absent: the producer does the work itself
//! ```
//!
//! A producer depends only on [`Sink`], and the work is a [`Consumer`]; neither
//! knows how the message travels. [`Memory`] carries it on channels inside this
//! process, sharded by a key so one key's messages keep their order while
//! different keys run in parallel. A broker realization (Amazon SQS, Cloudflare
//! Queues, Upstash `QStash`, Kafka) lives in its own crate: its publisher is a
//! [`Consumer`] placed behind [`Memory`], bounding `M` by the encoding it needs,
//! and its subscriber feeds the same domain [`Consumer`]. Nothing that offers or
//! consumes changes.
//!
//! # What a realization owes
//!
//! A consumer may see a message more than once: a broker redelivers, and
//! [`Memory`] retries. Consumers are therefore idempotent on the message's own
//! key, and a message a realization gives up on is handed to
//! [`Consumer::abandoned`], so the owning domain can leave it to its repair.

use std::fmt::Debug;

mod memory;

pub use self::memory::{Memory, Settings, Shard};

/// Whether a sink took a message.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer<M> {
    /// The sink owns delivery now.
    Accepted,
    /// The sink could not take it; the producer still owns the message.
    Refused(M),
}

/// What handling a message came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The work is done, or was already done.
    Done,
    /// The work could not be done now; a realization may try again.
    Retry,
}

/// Where a producer hands a message. Never waits: a realization that must
/// cross a network does so behind its own buffer, so the request path pays a
/// channel send at most.
pub trait Sink<M>: Send + Sync + Debug {
    /// Offers `message` for delivery, handing it back when it cannot be taken.
    fn offer(&self, message: M) -> Offer<M>;
}

/// The work a message stands for.
#[async_trait::async_trait]
pub trait Consumer<M: Send + Sync + 'static>: Send + Sync + Debug {
    /// Does the work `message` asks for. Called again for the same message
    /// after a [`Outcome::Retry`] or a broker's redelivery.
    async fn consume(&self, message: &M) -> Outcome;

    /// Told when a realization stops trying `message`.
    fn abandoned(&self, _message: &M) {}
}
