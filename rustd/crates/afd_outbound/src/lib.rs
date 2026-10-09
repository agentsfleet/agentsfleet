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
//! `afd_dragonfly::outbound` carries `provider` as a string and knows nothing about
//! what one is, so the report path enqueues an answer without a connector
//! anywhere in its graph. THIS crate is the only one that turns that string
//! into a [`afd_connector::Provider`] and picks a poster for it. Adding a
//! connector is therefore one arm in [`dispatch`] plus a sibling poster — never
//! a change to the path that produced the answer.
//!
//! # How the worker waits, and how it backs off
//!
//! **The read parks instead of polling.** A pooled connection borrowed per
//! command cannot be parked on a stream, so the worker owns an
//! [`afd_dragonfly::Dedicated`] connection and `XREADGROUP … BLOCK` holds until
//! an entry lands. An answer is delivered the instant it is queued, and an idle
//! deployment issues one command per block interval rather than a poll loop's
//! several a second forever.
//!
//! **The backoff is jittered.** A flat `200ms << attempt` would have every
//! worker that saw the same vendor outage retry in the same millisecond, and
//! the recovering vendor would be hit by the whole fleet at once. [`retry`]
//! uses `backon`'s jittered schedule instead, and Dimension 5.1 grades it.
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

mod abandon;
pub mod error;
pub mod interim;
pub mod lanes;
pub mod obligation;
pub mod poster;
pub mod producer;
pub mod retry;
pub mod slack;
pub mod worker;

pub use self::error::{Error, Result};
pub use self::interim::{Interim, Interjector};
pub use self::lanes::{Destination, IN_FLIGHT_DELIVERIES, LANE_DEPTH, Lanes};
pub use self::poster::{Attempt, Deliver, Posters, Verdict, deliver_with_retry, dispatch};
pub use self::slack::SlackPoster;
pub use self::worker::{BLOCK_INTERVAL, LONGEST_PARK, Worker};
