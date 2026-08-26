//! The `fleet.runners` row: enrolment, liveness, and the verdict that gates work.
//!
//! Split by concern rather than by length: [`validate`] is what an enrolment
//! must satisfy, [`bounds`] is what a HOST may assert about itself, [`token`]
//! is the credential, [`reconcile`] is the pure verdict, [`policy`] reads the
//! row the assignment is stored in and [`spelling`] writes it — and
//! [`store`], [`record`] and [`heartbeat`] are the only modules that touch
//! Postgres. The Zig equivalent is one handler
//! per verb, each holding a request context and a pool, so the decision and the
//! response are the same function and neither can be tested without the other.
//!
//! Everything above `store` is pure, which is what puts the whole verdict
//! matrix, both bound sets and every decode branch in a unit test with no
//! datastore anywhere near it.

pub mod admin;
pub mod bounds;
pub mod heartbeat;
pub mod policy;
pub mod reconcile;
pub mod record;
pub mod spelling;
pub mod store;
pub mod token;
pub mod validate;

pub use self::heartbeat::{Beat, NO_REPORT};
pub use self::policy::{AssignmentColumns, StoredVerdict};
pub use self::record::SelfRow;
pub use self::store::{Enrolled, Runners};
