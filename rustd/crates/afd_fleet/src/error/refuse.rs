//! The six refusals a lease's own lifecycle can answer with.
//!
//! Split from [`super`] because they are a different KIND of failure from
//! everything beside them there. Every other constructor in that module reports
//! something that went wrong — a datastore that would not answer, a column that
//! will not parse, an envelope missing a field. These six report nothing wrong
//! at all: the system worked exactly as designed and the answer is no.
//!
//! They also share a shape none of the others do. Each names ONE outcome of one
//! guarded statement, the statement has already decided it, and there is
//! nothing left for a call site to add — so none of them takes a payload. A
//! `detail` parameter on any of these would be an invitation to describe the
//! same refusal differently at a second site, which is the failure
//! [`super::detail`] exists to prevent.

use super::{Error, ErrorKind};

/// Refuses a report from a holder the fleet has already superseded.
///
/// The six builders here and below carry no payload, which is what distinguishes
/// them from every other kind in this file: each names ONE outcome of one
/// guarded statement, and the statement has already decided it. There is nothing
/// left for a call site to add, and a `detail` parameter would be an invitation
/// to describe the same refusal differently at a second site.
pub(crate) fn stale_fence() -> Error {
    Error::new(ErrorKind::StaleFence)
}

/// Refuses a lease id that is not this runner's, or is not a lease at all.
pub(crate) fn lease_not_found() -> Error {
    Error::new(ErrorKind::LeaseNotFound)
}

/// Refuses a renewal for a lease that has moved on.
pub(crate) fn lease_lost() -> Error {
    Error::new(ErrorKind::LeaseLost)
}

/// Refuses a renewal past the hard runtime ceiling.
pub(crate) fn lease_max_runtime() -> Error {
    Error::new(ErrorKind::LeaseMaxRuntime)
}

/// Refuses a renewal the tenant's balance cannot fund.
pub(crate) fn renewal_no_credits() -> Error {
    Error::new(ErrorKind::RenewalNoCredits)
}

/// Refuses a renewal against the fleet's own declared ceiling.
pub(crate) fn budget_exhausted() -> Error {
    Error::new(ErrorKind::BudgetExhausted)
}
