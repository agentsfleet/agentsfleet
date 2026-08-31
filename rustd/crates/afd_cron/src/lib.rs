//! Fleet schedules: the stored intent, the scheduler it is reconciled against,
//! and the signed fire that wakes a fleet.
//!
//! # This daemon owns no timer, and that is the design
//!
//! Nothing here sleeps, ticks, or wakes on a clock. A schedule is INTENT stored
//! in Postgres and mirrored to an external scheduler, and a fleet runs when that
//! scheduler posts a signed callback to `/v1/ingress/qstash/schedules`. Two
//! daemons behind one load balancer therefore cannot double-fire a schedule,
//! because neither of them is counting — and a daemon that was down for an hour
//! does not wake up owing an hour of catch-up fires.
//!
//! ```text
//!   person ──► store          intent, claimed by the caller that will push it
//!                │
//!                ▼
//!              qstash         upsert / remove, and the answer classified
//!                │
//!                ▼
//!              finalize       fence released, synced or failed-with-a-reason
//!
//!   scheduler ──► verifier    HS256 over two keys, body bound into the token
//!                   │
//!                   ▼
//!                 fire        claim + XADD, atomically, at most once
//! ```
//!
//! # The two halves never share a decision
//!
//! [`validate`] and [`verifier`] decide with no datastore in reach, and
//! [`store`] reads and writes with no opinion about what is valid. That is what
//! keeps the grammar and the signature check provable without an environment —
//! and a grammar that needed a Postgres to test is one that goes untested.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod fire;
mod service;
mod store;

pub mod error;
pub mod model;
pub mod qstash;
pub mod sql;
pub mod validate;
pub mod verifier;

pub use self::error::{Error, Result};
pub use self::fire::{Fire, Fired};
pub use self::model::{DesiredStatus, MAX_SCHEDULES_PER_FLEET, Schedule, Source, SyncStatus};
pub use self::qstash::QStash;
pub use self::service::{Reconciled, Schedules as ScheduleService};
pub use self::store::{Change, FireTarget, NewSchedule, Refused, SYNC_LEASE_MS, Schedules};
pub use self::validate::Invalid;
pub use self::verifier::{SigningKeys, Unverified, VerifiedFire};
