//! Every statement the run plane runs.
//!
//! One `sql` module per crate-to-be rather than one for the whole of
//! `afd_fleet`: RULE SQLMOD puts a statement with the code that runs it, and a
//! shared module was the last thing tying four planes to one compilation unit.
//! `vault`, `provider`, `memory` and `gate` each took their own the same way.

pub mod context;
pub mod draft;
pub mod fleet;
pub mod gates;
pub mod history;
pub mod meter;
pub mod renew;
pub mod report;
pub mod report_statement;
pub mod resume;
pub mod run;
pub mod shared;
pub mod standing;
pub mod tool_detail;

pub mod queue;

pub(crate) use afd_state::sql::{
    FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE, RUN_STATUS_EXPIRED, RUN_STATUS_REPORTED,
    RUN_STATUS_STOPPED, RUNNER_STATUS_ACTIVE,
};
