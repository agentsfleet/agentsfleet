//! The runner control plane: what a host may do, and what it is owed.
//!
//! # Why this is not a module inside `afd_fleet`
//!
//! It was, and nothing about a RUNNER required it to be. `afd_fleet` is the
//! run's plane — leases, gates, the vault a run opens — and this crate is the
//! HOST's: enrolment, liveness, the verdict matrix over what a machine may
//! assert about itself, and the sweepers that notice when one goes quiet.
//! The two meet at exactly one point, the `lease_released` audit row the
//! lease's finalize writes, and that meeting is a vocabulary import from
//! [`sql`] rather than a shared module.
//!
//! # Shape
//!
//! Split by concern rather than by length: [`validate`] is what an enrolment
//! must satisfy, [`bounds`] is what a HOST may assert about itself,
//! [`reconcile`] is the pure verdict, [`policy`] reads the row the assignment
//! is stored in and [`spelling`] writes it — and [`store`], [`record`] and
//! [`heartbeat`] are the only modules that touch Postgres. Everything above
//! `store` is pure, which is what puts the whole verdict matrix, both bound
//! sets and every decode branch in a unit test with no datastore anywhere
//! near it.

mod error;

pub mod admin;
pub mod bounds;
pub mod heartbeat;
pub mod policy;
pub mod reconcile;
pub mod record;
mod rotate;
pub mod spelling;
pub mod sql;
pub mod store;
pub mod sweep;
pub mod validate;
pub mod view;

pub use self::admin::{PolicyAssigned, SelftestRequested};
pub use self::error::{
    DETAIL_DATABASE_ERROR, DETAIL_HOST_ID_BOUNDS, DETAIL_REGISTRY_ALLOWLIST,
    DETAIL_RUNNER_NOT_FOUND, DETAIL_RUNNER_NOT_REVOKED, DETAIL_RUNNER_STILL_LEASED,
    DETAIL_SELFTEST_REFUSED, Error, Result,
};
pub use self::heartbeat::{Beat, NO_REPORT};
pub use self::policy::{AssignmentColumns, StoredVerdict};
pub use self::reconcile::{Verdict, reconcile};
pub use self::record::SelfRow;
pub use self::store::{Enrolled, Runners};
pub use self::view::{
    KeysetCursor, PageLimit, RunnerDetail, RunnerEventFilter, RunnerEventPage, RunnerItem,
    RunnerPage,
};
