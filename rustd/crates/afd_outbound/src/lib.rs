//! Delivering a fleet's answer back to the connector the question came from.
//!
//! ```text
//!   report path ──► OutboundQueue::enqueue     provider as opaque text
//!                        │
//!                        ▼   connector:outbound  (durable, consumer-grouped)
//!                   Worker::run
//!                        │  read_pending  ── anything a previous process was
//!                        │                   handed and never acknowledged
//!                        │  read_blocking ── parked on the stream, raced
//!                        │                   against the supervisor's token
//!                        ▼
//!                   dispatch by provider ──► the poster ──► the vendor
//!                        │
//!                        ▼
//!                   ack, exactly once
//! ```
//!
//! # Invariant 9 lives at this crate's boundary
//!
//! `afd_redis::outbound` carries `provider` as a string and knows nothing about
//! what one is, so the report path enqueues an answer without a connector
//! anywhere in its graph. THIS crate is the only one that turns that string
//! into a [`afd_connector::Provider`] and picks a poster for it. Adding a
//! connector is therefore one arm in [`dispatch`] plus a sibling poster — never
//! a change to the path that produced the answer. `worker.zig` states the same
//! rule and is the only importer of `slack/post.zig` for the same reason.
//!
//! # Two recorded departures from the Zig
//!
//! **The read parks instead of polling.** `worker.zig` sleeps 250 ms between
//! non-blocking claims, and says why: its pooled connections are borrowed
//! per-command and cannot be parked on a stream. That is a fact about a
//! blocking client, not about the queue — so here the worker owns an
//! [`afd_redis::Dedicated`] connection and `XREADGROUP … BLOCK` holds until an
//! entry lands. An answer is delivered the instant it is queued rather than up
//! to a quarter-second later, and an idle deployment issues one command per
//! block interval instead of four per second forever.
//!
//! **The backoff is jittered.** The Zig retries at a flat `200ms << attempt`,
//! so every worker that saw the same vendor outage retries in the same
//! millisecond and the recovering vendor is hit by the whole fleet at once.
//! [`retry`] uses `backon`'s jittered schedule instead. This is an improvement
//! over the port, not parity with it, and Dimension 5.1 grades the improved
//! behaviour.
//!
//! # Delivery is serial, and that is a requirement
//!
//! One job at a time, start to finish. Two answers into one Slack thread must
//! arrive in the order the fleet produced them, and nothing downstream
//! reorders them back. The throughput ceiling that buys is real and is the
//! right trade: answers arrive at model-run cadence, and a second worker would
//! be a second consumer name, not more parallelism within one.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod error;
pub mod poster;
pub mod retry;
pub mod slack;
pub mod worker;

pub use self::error::{Error, Result};
pub use self::poster::{Deliver, Posters, Verdict, deliver_with_retry, dispatch};
pub use self::slack::SlackPoster;
pub use self::worker::{BLOCK_INTERVAL, Worker};
