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

const BASE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const INCREMENTAL_BYTES: u64 = 1024 * 1024 * 1024;
const WORKSPACE_BYTES: i64 = 50 * 1024 * 1024 * 1024;
const TRANSFER_SECONDS: i32 = 15 * 60;
const PIN_SECONDS: i32 = 30 * 60;
const URL_SECONDS: u64 = 5 * 60;
const ORPHAN_SECONDS: i32 = 60 * 60;
const GC_BATCH: i64 = 256;
const LIVE_GENERATIONS: i64 = 3;
const TERMINAL_SECONDS: i32 = 7 * 24 * 60 * 60;
