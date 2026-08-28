//! Every statement the lease plane runs.
//!
//! One `sql` module per crate-to-be rather than one for the whole of
//! `afd_fleet`: RULE SQLMOD puts a statement with the code that runs it, and a
//! shared module was the last thing tying four planes to one compilation unit.
//! `vault`, `provider`, `memory` and `gate` each took their own the same way.

pub mod activity;
pub mod fleet;
pub mod lease;
pub mod renew;
pub mod report;
pub mod session;

pub use afd_state::sql::{
    ADMIN_STATE_ACTIVE, ADMIN_STATE_DRAINED, ADMIN_STATE_DRAINING, LAST_SEEN_NEVER,
    LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, LEASE_STATUS_REPORTED,
};
