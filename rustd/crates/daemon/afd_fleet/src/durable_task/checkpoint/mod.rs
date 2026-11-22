//! Fenced plans, immutable object custody and bounded recovery on existing runs.

mod commit;
mod effective;
mod guard;
mod identity;
mod maintenance;
mod object_row;
mod objects;
mod page;
mod pins;
mod plan;
mod row;
mod transfer;

pub(super) use commit::publish;
pub(crate) use identity::{load as content_identity, quota as lock_quota};
pub(super) use maintenance::maintain;
pub(crate) use object_row::Object as StoredObject;
pub(super) use objects::register;
pub(super) use page::{read, write};
pub(super) use pins::renew;
pub(super) use plan::create;
pub(crate) use transfer::{Verification, verify_object};
pub(super) use transfer::{authorize, list, uploads};

/// The statements a run's turn checkpoints share with task checkpoints,
/// included once so the two custody paths cannot drift apart.
pub(crate) const QUOTA_RESERVE: &str = include_str!("sql/quota_reserve.sql");
pub(crate) const OBJECTS_INSERT: &str = include_str!("sql/objects_insert.sql");
pub(crate) const DESCRIPTORS: &str = include_str!("sql/descriptors.sql");
pub(crate) const VERIFY: &str = include_str!("sql/verify.sql");

use std::time::Duration;

/// How long a checkpoint's uploads or restore may run in total, by default.
pub(crate) const TRANSFER: Duration = Duration::from_mins(15);
/// How long a restoring checkpoint is pinned, by default.
pub(crate) const PIN: Duration = Duration::from_mins(30);
/// How long a runner's presigned upload or download link stays valid, by default.
pub(crate) const LINK: Duration = Duration::from_mins(5);
/// How long an uncommitted upload is kept, by default.
pub(crate) const ORPHAN: Duration = Duration::from_hours(1);
/// Objects deleted per storage call, by default.
pub(crate) const GC_BATCH: u32 = 256;
/// Newest checkpoints kept per task, by default.
pub(crate) const LIVE_GENERATIONS: u32 = 3;
/// How long a finished task's records are kept, by default.
pub(crate) const TERMINAL: Duration = af_core::timing::DAY.saturating_mul(7);
