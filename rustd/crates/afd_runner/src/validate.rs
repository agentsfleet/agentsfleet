//! What an enrolment request must satisfy before a row is written.
//!
//! Every bound is declared on the wire type with garde
//! (`afd_wire::runner::{RegisterRequest, AssignedPolicy, ExtraBind}`); what
//! stays here is the sentence each broken bound earns, keyed by the path garde
//! reports, and the worker clamp. The host-id and allowlist sentences are
//! `protocol_policy.zig`'s and `register.zig`'s, pinned byte-for-byte — a
//! client reads them.

use afd_core::limits::WorkerCount;
use afd_validate::Sentences;
use afd_wire::runner::{
    AssignedPolicy, BIND_NOTE_MAX_BYTES, BIND_PATH_MAX_BYTES, BIND_PATH_MIN_BYTES, EXTRA_BINDS_MAX,
    LABEL_MAX_BYTES, LABELS_MAX, RegisterRequest,
};
use const_format::concatcp;
use garde::Validate as _;

use crate::error::{DETAIL_HOST_ID_BOUNDS, DETAIL_REGISTRY_ALLOWLIST, Result, rejected};

/// The refusal for a bind path that is relative, non-canonical, out of
/// bounds, or overlapping a daemon-owned or sensitive subtree.
pub const DETAIL_EXTRA_BINDS: &str = concatcp!(
    "extra_binds paths must be absolute host paths of ",
    BIND_PATH_MIN_BYTES,
    "-",
    BIND_PATH_MAX_BYTES,
    " bytes outside the daemon-owned baseline and the sensitive set, with no traversal"
);

/// The refusal for an assignment adding too many binds.
pub const DETAIL_EXTRA_BINDS_COUNT: &str =
    concatcp!("extra_binds holds at most ", EXTRA_BINDS_MAX, " entries");

/// The refusal for a bind note past its bound.
pub const DETAIL_EXTRA_BIND_NOTE: &str = concatcp!(
    "extra_binds notes must be at most ",
    BIND_NOTE_MAX_BYTES,
    " bytes"
);

/// The refusal for too many labels, or one past its bound.
pub const DETAIL_LABELS: &str = concatcp!(
    "labels holds at most ",
    LABELS_MAX,
    " entries of at most ",
    LABEL_MAX_BYTES,
    " bytes each"
);

const PATH_HOST_ID: &str = "host_id";
const PATH_LABELS: &str = "labels";
const PATH_LABEL: &str = "labels[]";
const PATH_ALLOWLIST: &str = "registry_allowlist";
const PATH_ALLOWLIST_ENTRY: &str = "registry_allowlist[]";
const PATH_BINDS: &str = "extra_binds";
const PATH_BIND_NOTE: &str = "extra_binds[].note";
const PATH_BIND_PATH: &str = "extra_binds[].path";
const PATH_POLICY_ALLOWLIST: &str = "assigned_policy.registry_allowlist";
const PATH_POLICY_ALLOWLIST_ENTRY: &str = "assigned_policy.registry_allowlist[]";
const PATH_POLICY_BINDS: &str = "assigned_policy.extra_binds";
const PATH_POLICY_BIND_NOTE: &str = "assigned_policy.extra_binds[].note";
const PATH_POLICY_BIND_PATH: &str = "assigned_policy.extra_binds[].path";

/// The sentences an assignment earns, validated on its own by the operator's
/// assign-policy verb. Table order is the order the checks read in before
/// they moved onto the type: the allowlist, then the binds.
const POLICY: Sentences = Sentences::new(
    &[
        (PATH_ALLOWLIST, DETAIL_REGISTRY_ALLOWLIST),
        (PATH_ALLOWLIST_ENTRY, DETAIL_REGISTRY_ALLOWLIST),
        (PATH_BINDS, DETAIL_EXTRA_BINDS_COUNT),
        (PATH_BIND_NOTE, DETAIL_EXTRA_BIND_NOTE),
        (PATH_BIND_PATH, DETAIL_EXTRA_BINDS),
    ],
    DETAIL_EXTRA_BINDS,
);

/// The sentences a whole enrolment earns, where `dive` reports the assignment
/// under `assigned_policy`. The host id answers first, as it always did.
const REGISTRATION: Sentences = Sentences::new(
    &[
        (PATH_HOST_ID, DETAIL_HOST_ID_BOUNDS),
        (PATH_POLICY_ALLOWLIST, DETAIL_REGISTRY_ALLOWLIST),
        (PATH_POLICY_ALLOWLIST_ENTRY, DETAIL_REGISTRY_ALLOWLIST),
        (PATH_POLICY_BINDS, DETAIL_EXTRA_BINDS_COUNT),
        (PATH_POLICY_BIND_NOTE, DETAIL_EXTRA_BIND_NOTE),
        (PATH_POLICY_BIND_PATH, DETAIL_EXTRA_BINDS),
        (PATH_LABELS, DETAIL_LABELS),
        (PATH_LABEL, DETAIL_LABELS),
    ],
    DETAIL_EXTRA_BINDS,
);

/// The assignment as it will be STORED, with the worker count clamped.
///
/// Returned rather than mutated in place so the caller cannot forget to use it:
/// what is echoed to the enrolling operator must be what the host will apply,
/// and a clamp written back into the request would leave two values in scope
/// with only a comment saying which is authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredAssignment {
    /// The clamped worker ceiling.
    pub worker_count: WorkerCount,
}

impl StoredAssignment {
    /// What a proved assignment stores. Clamped, never refused:
    /// `register.zig` clamps into the shared bounds so what is echoed is what
    /// runs, and `WorkerCount::clamping` is that rule as a type.
    fn of(policy: &AssignedPolicy<'_>) -> Self {
        Self {
            worker_count: WorkerCount::clamping(policy.worker_count),
        }
    }
}

/// Proves an assignment's bounds and resolves what will actually be stored.
///
/// # Errors
/// Refuses the first broken bound with its sentence: the allowlist's count or
/// grammar, the bind count, a bind note, or a bind path.
pub fn assignment(policy: &AssignedPolicy<'_>) -> Result<StoredAssignment> {
    policy
        .validate()
        .map_err(|report| rejected(POLICY.pick(&report)))?;
    Ok(StoredAssignment::of(policy))
}

/// Proves a whole enrolment — host id, assignment and labels — and resolves
/// the assignment that will be stored.
///
/// # Errors
/// Refuses the first broken bound with its sentence, the host id first.
pub fn registration(request: &RegisterRequest<'_>) -> Result<StoredAssignment> {
    request
        .validate()
        .map_err(|report| rejected(REGISTRATION.pick(&report)))?;
    Ok(StoredAssignment::of(&request.assigned_policy))
}

#[cfg(test)]
#[path = "validate/tests.rs"]
mod tests;
