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
//! [`support`] carries the request shapes. The refusal vocabulary and the
//! renderings are `afd_http::handler::schedule`'s, shared with the runner's
//! schedules verb, because a surface that tells a caller "no schedule with that
//! identifier" in two spellings has two answers to the same question.

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
