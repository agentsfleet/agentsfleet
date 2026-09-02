//! `/v1/workspaces/{id}/fleets/{fleet_id}/schedules` — the CRUD half of §3.
//!
//! # A create answers 201 even when the scheduler refused
//!
//! The row is saved either way, and the answer carries the sync state so the
//! caller can see which happened. Answering 502 for an upstream refusal would
//! be telling a person their schedule was not created when it was — and the
//! next sync would then repair a schedule they believe does not exist.
//!
//! # A delete does not delete
//!
//! It sets `desired_status = deleting` and pushes. The row goes only once the
//! external scheduler has confirmed, because a row removed first would leave a
//! schedule firing at a callback this daemon can no longer resolve to a fleet —
//! see [`afd_cron::DesiredStatus::Deleting`].
//!
//! # Where the verbs live
//!
//! [`read`] answers from this daemon's own rows and touches nothing upstream;
//! [`write`] holds the four that reconcile against the external scheduler; and
//! [`support`] carries the request shapes and the renderings both halves go
//! through. The refusal vocabulary stays here rather than beside its callers,
//! because a surface that tells a caller "no schedule with that identifier" in
//! two spellings has two answers to the same question.

pub(crate) mod read;
mod support;
pub(crate) mod write;

pub(crate) use self::read::{list, one};
pub(crate) use self::write::{create, patch, purge, sync};

/// The scoped event a failed schedule read is logged under.
const EVENT_READ: &str = "schedule_read_failed";

/// The scoped event a failed schedule write is logged under.
const EVENT_WRITE: &str = "schedule_write_failed";

/// The refusal a body this route cannot read as a schedule earns.
const DETAIL_INVALID_BODY: &str = "The request body is not a schedule this daemon can read.";

/// The refusal an expression this daemon will not register earns.
const DETAIL_INVALID_CRON: &str =
    "The cron expression must be five numeric fields this daemon accepts.";

/// The refusal a zone this daemon will not pass upstream earns.
const DETAIL_INVALID_TIMEZONE: &str = "The timezone is not a name this daemon will register.";

/// The refusal a message that would wake a fleet with nothing earns.
const DETAIL_INVALID_MESSAGE: &str = "The message must not be empty.";

/// The refusal a fleet at its schedule ceiling earns.
const DETAIL_TOO_MANY: &str = "This fleet already holds as many schedules as it may.";

/// The refusal a duplicate upstream key earns.
const DETAIL_DUPLICATE: &str = "This fleet already has a schedule under that key.";

/// The refusal a schedule this fleet does not hold earns.
const DETAIL_NOT_FOUND: &str = "No schedule with that identifier belongs to this fleet.";

/// The refusal a schedule another syncer is holding earns.
///
/// A conflict rather than a not-found, because the row EXISTS and the caller
/// may retry in a moment — the two answers send a caller to different places.
const DETAIL_HELD: &str = "This schedule is being synchronised. Try again in a moment.";
