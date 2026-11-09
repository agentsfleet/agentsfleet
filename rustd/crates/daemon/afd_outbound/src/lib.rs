//! Delivering a fleet's answer back to the connector the question came from.
//!
//! ```text
//!   report path ──► obligation::owe (inside the report's transaction)
//!                        │  then a lossy af.outbound.wake ping
//!                        ▼
//!                   Worker::run ── claims lane heads from fleet.obligations
//!                        │         on the ping, or every CLAIM_PASS
//!                        ▼
//!                   dispatch by provider ──► the poster ──► the vendor
//!                        │
//!                        ▼
//!                   stamp, release or abandon, matching the holder's token
//! ```
//!
//! # Invariant 9 lives at this crate's boundary
//!
//! The ledger carries `provider` as a string, so the report path owes an
//! answer without a connector anywhere in its graph. THIS crate is the only
//! one that turns that string into a [`afd_connector::Provider`] and picks a
//! poster for it. Adding a connector is therefore one arm in [`dispatch`] plus
//! a sibling poster — never a change to the path that produced the answer.
//!
//! # The backoff is jittered
//!
//! A flat `200ms << attempt` would have every worker that saw the same vendor
//! outage retry in the same millisecond. [`retry`] uses `backon`'s jittered
//! schedule instead.
//!
//! # Delivery is serial per lane, and that is a requirement
//!
//! Two answers into one Slack thread must arrive in the order the fleet
//! produced them. The claim hands out at most one row per (provider,
//! workspace) lane at a time, across every replica, so no second replica can
//! overtake the first inside one thread.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod abandon;
pub mod error;
pub mod interim;
pub mod lanes;
pub mod obligation;
pub mod poster;
pub mod retry;
pub mod slack;
pub mod worker;

pub use self::error::{Error, Result};

/// Statements a suite reads plans of, bound rather than re-exported (RS-TESTCONST).
#[cfg(feature = "test-util")]
pub mod test_util {
    /// The lane-head claim, for its query-plan test.
    pub const CLAIM_HEADS: &str = crate::obligation::sql::CLAIM_HEADS;
}
pub use self::interim::{Interim, Interjector};
pub use self::lanes::{Destination, IN_FLIGHT_DELIVERIES, LANE_DEPTH, Lanes};
pub use self::obligation::{Claimed, Outgoing};
pub use self::poster::{Attempt, Deliver, Posters, Verdict, deliver_with_retry, dispatch};
pub use self::slack::SlackPoster;
pub use self::worker::Worker;
