//! Fenced plans, immutable object custody and bounded recovery on existing leases.

mod guard;
mod identity;
mod objects;
mod object_row;
mod page;
mod plan;
mod row;
mod pins;
mod transfer;
mod commit;
mod effective;
mod maintenance;

pub(super) use plan::create;
pub(super) use objects::register;
pub(super) use page::{read, write};
pub(super) use pins::renew;
pub(super) use transfer::{authorize, uploads, list};
pub(super) use commit::publish;
pub(super) use maintenance::maintain;

const BASE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const INCREMENTAL_BYTES: u64 = 1024 * 1024 * 1024;
const WORKSPACE_BYTES: i64 = 50 * 1024 * 1024 * 1024;
const TRANSFER_SECONDS: i32 = 15 * 60;
const PIN_SECONDS: i32 = 30 * 60;
const URL_SECONDS: i32 = 5 * 60;
const ORPHAN_SECONDS: i32 = 60 * 60;
const GC_BATCH: i64 = 256;
const LIVE_GENERATIONS: i64 = 3;
const TERMINAL_SECONDS: i32 = 7 * 24 * 60 * 60;
