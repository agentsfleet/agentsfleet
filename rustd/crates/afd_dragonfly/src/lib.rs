//! Dragonfly cluster access for outbound delivery, live activity, sessions and wake notifications.
//! Blocking consumers own separate connections so parked reads cannot stall ordinary commands.

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
#![deny(clippy::self_named_module_files)]

pub mod capacity;
pub mod client;
pub mod config;
pub mod dedicated;
pub mod doorbell;
pub mod error;
pub mod hub;
pub mod kv;
mod namespace;
pub mod outbound;
pub mod preflight;
pub mod session;
pub mod streams;
#[cfg(feature = "test-util")]
pub mod test_util;
pub(crate) mod topology;
pub(crate) mod transport;

pub use afd_core::env::EnvSource;

pub use crate::capacity::Capacity;
pub use crate::client::Dragonfly;
pub use crate::config::{DragonflyConfig, DragonflyRole};
pub use crate::dedicated::Dedicated;
pub use crate::doorbell::Doorbell;
pub use crate::error::Error;
pub use crate::hub::{Message, Subscription, SubscriptionHub, production_backoff};
pub use crate::namespace::NAMESPACE_KNOB;
pub use crate::outbound::{
    OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OutboundDelivery, OutboundJob, OutboundQueue,
    OutboundReader, outbound_consumer,
};
pub use crate::session::{
    AbortOutcome, AbortReason, Approval, ApproveOutcome, SessionState, SessionStatus, SessionStore,
    VerifyOutcome, VerifyPayload,
};
pub use crate::streams::{
    ACKNOWLEDGED_HISTORY, Backlog, EventId, FleetStreams, Trimmed, fleet_activity_channel,
};
