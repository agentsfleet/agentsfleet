//! The datastore for `agentsfleetd`: event streams, the subscription hub, the
//! session store, and the readiness index, on a Dragonfly cluster.
//!
//! # One transport, and why it is the cluster one
//!
//! Redis and Dragonfly speak the same protocol, so the only axis this crate
//! can see is standalone versus cluster — and the daemon's datastore is a
//! cluster. There is no standalone path and no selector between the two:
//! every connection is a redis-rs `ClusterConnection` over RESP3, built in
//! `transport`, routing each command by its key's slot and following a
//! `MOVED` while a slot migrates. A seed that is not a cluster refuses boot.
//!
//! # Three connections, not a pool
//!
//! The driver keeps one socket per node and applies one reply deadline per
//! connection, so a parked read on a shared handle would hold the owning
//! node's only socket and impose the park-sized deadline on everyone. So
//! [`client::Redis`] is the shared handle for request-path commands,
//! [`dedicated::Dedicated`] is the one a blocking consumer owns alone, and
//! [`hub::SubscriptionHub`] owns the pub/sub one (exactly one per process,
//! Invariant 2) and multiplexes readers locally.
//!
//! # Every multi-key operation shares a slot or does not exist
//!
//! The at-most-once append is the crate's only two-key script, and its marker
//! key carries the stream key as its hash tag — see `streams/once.rs`. Every
//! other script is single-key, and `SCAN` fans out over every primary the
//! cluster names because a walk has no key to route by.
//!
//! # What is shared with the Zig daemon, and why
//!
//! Both binaries read and write the same keys. So the key shapes
//! (`fleet:{id}:events`, `fleet:ready`, `auth:session:{id}`), the consumer
//! group name, the stream trim, and the session time-to-live are a DATA FORMAT
//! and are spelled here exactly as they are there. The atomic session
//! transition goes further: `session_verify_consume.lua` is included from the
//! Zig tree byte-for-byte, so the two binaries send the same script rather
//! than two implementations that agree today.

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
pub mod dedicated;
pub mod error;
pub mod hub;
pub mod kv;
pub mod outbound;
pub mod ready;
pub mod session;
pub mod streams;
#[cfg(feature = "test-util")]
pub mod test_util;
pub(crate) mod topology;
pub(crate) mod transport;

pub use afd_core::env::EnvSource;

pub use crate::client::Redis;
pub use crate::config::{RedisConfig, RedisRole};
pub use crate::dedicated::Dedicated;
pub use crate::error::Error;
pub use crate::hub::{Message, Subscription, SubscriptionHub, production_backoff};
pub use crate::outbound::{
    OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OutboundDelivery, OutboundJob, OutboundQueue,
    OutboundReader, outbound_consumer,
};
pub use crate::ready::{Ready, ReadyIndex, ReadyToken};
pub use crate::session::{
    AbortOutcome, AbortReason, Approval, ApproveOutcome, SessionState, SessionStatus, SessionStore,
    VerifyOutcome, VerifyPayload,
};
pub use crate::streams::{
    EventId, FleetEvent, FleetStreams, fleet_activity_channel, fleet_stream_key,
};
