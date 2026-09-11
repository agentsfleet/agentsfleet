//! Immutable, revision-bound evidence for datastore benchmark campaigns.
//!
//! A lane still writes its convenient fixed result path. Capture immediately
//! copies those bytes into a unique campaign/lane/sample directory and writes
//! a sidecar that binds them to source, topology and resources. The grader
//! reads only the archive; a later fixed-path overwrite cannot alter history.

mod capture;
mod context;
mod git;
mod grade;
mod model;

pub use capture::{capture, prepare};
pub use grade::{Grade, grade};
pub use model::{BaselinePlan, CAMPAIGN_ROOT, DEFAULT_PLAN_PATH, Sidecar};

#[cfg(test)]
mod tests;
