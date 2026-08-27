//! The runner control plane: what a host may do, and what it is owed.
//!
//! Named for `src/agentsfleetd/fleet/`, which is where the Zig daemon keeps
//! this domain. What lands first is the runner ROW — enrolment, liveness, and
//! the degraded verdict — because every other verb in the plane is gated on a
//! runner the authenticator has already proven and this crate has to be able to
//! describe.
//!
//! # What this crate is, against what the Zig equivalent is
//!
//! `service.zig` and its siblings are handlers: they hold a request context,
//! reach into a pool, write a response, and log on the way past. The seam
//! between "decide" and "answer over HTTP" does not exist there, which is why
//! `assign.select` can swallow a Postgres failure and return the same `null` an
//! idle fleet returns.
//!
//! Here the seam is the crate boundary. Nothing in `afd_fleet` names axum, a
//! status code, or a response body; every operation answers a value or
//! [`Error`], and `afd_api` decides what that becomes on the wire. The
//! behaviour is unchanged — a transient failure still answers no-work with a
//! backoff hint, which is Zig parity — but it is decided once, where it can be
//! read, rather than at each `catch`.
//!
//! # Where the SQL lives
//!
//! In [`sql`], collected, because the only enforcement of verbatim-SQL parity
//! with the Zig daemon is REVIEW reading the two side by side. That module's
//! documentation carries the full reasoning, including why `core_api`'s
//! inline-SQL shape does not transfer.

// A dependency listed but unused is supply-chain surface and compile time for
// nothing. Gated on `not(test)` because the test build links dev-dependencies
// into this same target, where a test-only crate legitimately goes unused by
// the library's own code.
#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod bundle;
pub mod credential;
pub mod error;
pub mod gate;
pub mod lease;
pub mod memory;
pub mod money;
pub mod policy;
pub mod provider;
pub mod runner;
pub mod secrets;
pub mod sql;
pub mod streams;
pub mod sweep;
pub mod vault;

pub use crate::error::{Error, Result};
pub use crate::runner::admin::{PolicyAssigned, SelftestRequested};
pub use crate::runner::reconcile::{Verdict, reconcile};
pub use crate::runner::view::{
    KeysetCursor, PageLimit, RunnerDetail, RunnerEventFilter, RunnerEventPage, RunnerItem,
    RunnerPage,
};
pub use crate::runner::{
    AssignmentColumns, Beat, Enrolled, NO_REPORT, Runners, SelfRow, StoredVerdict,
};
