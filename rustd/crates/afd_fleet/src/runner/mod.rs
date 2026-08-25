//! The `fleet.runners` row: enrolment, liveness, and the verdict that gates work.
//!
//! Split by concern rather than by length: [`validate`] is what a request must
//! satisfy, [`token`] is the credential, [`reconcile`] is the pure verdict, and
//! [`store`] is the only module that touches Postgres. The Zig equivalent is one
//! handler per verb, each holding a request context and a pool — so the decision
//! and the response are the same function and neither can be tested without the
//! other.

pub mod reconcile;
pub mod record;
pub mod store;
pub mod token;
pub mod validate;

pub use self::record::SelfRow;
pub use self::store::{Enrolled, Runners};
