//! Redis for `agentsfleetd`: event streams, the subscription hub, the session
//! store, and the readiness index.
//!
//! # One connection does the work of a pool
//!
//! The Zig daemon hand-rolls a connection pool, RESP framing and a reconnect
//! loop across roughly three thousand lines under `src/agentsfleetd/queue/`,
//! because a blocking client can only have one command in flight per socket.
//! An async client has no such limit: [`client::Redis`] holds one multiplexed
//! connection that carries concurrent commands and routes each reply back to
//! whoever is waiting. What the pool existed to solve stops being a problem
//! rather than being ported.
//!
//! Pub/sub is the exception, and it gets its own connection — exactly one per
//! process ([`hub::SubscriptionHub`], Invariant 2), because `SUBSCRIBE` takes a
//! socket over and a connection per reader makes a browser tab a socket.
//!
//! # What is shared with the Zig daemon, and why
//!
//! Both binaries read and write the same Redis. So the key shapes
//! (`fleet:{id}:events`, `fleet:ready`, `auth:session:{id}`), the consumer
//! group name, the stream trim, and the session time-to-live are a DATA FORMAT
//! and are spelled here exactly as they are there. The atomic session
//! transition goes further: `session_verify_consume.lua` is included from the
//! Zig tree byte-for-byte, so the two binaries send the same script rather than
//! two implementations that agree today.

// Same reasoning as the sibling crates: an unused dependency is supply-chain
// surface and compile time for nothing.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
// Every duplicate in this crate's graph is inside its dependencies', not ours —
// `redis` and `rustls` pull the older RustCrypto and getrandom lines. This is
// `expect`, so it fails the build once that stops being true.
#![expect(
    clippy::multiple_crate_versions,
    reason = "redis and rustls pin transitive versions this workspace does not choose"
)]

pub mod client;
pub mod config;
pub mod error;
pub mod hub;
pub mod kv;
pub mod ready;
pub mod session;
pub mod streams;

pub use afd_core::env::EnvSource;

pub use crate::client::Redis;
pub use crate::config::{RedisConfig, RedisRole};
pub use crate::error::Error;
pub use crate::hub::{Backoff, Message, Subscription, SubscriptionHub};
pub use crate::ready::{Ready, ReadyIndex, ReadyToken};
pub use crate::session::{
    AbortOutcome, AbortReason, Approval, ApproveOutcome, SessionState, SessionStatus, SessionStore,
    VerifyOutcome, VerifyPayload,
};
pub use crate::streams::{EventId, FleetEvent, FleetStreams, fleet_activity_channel};
