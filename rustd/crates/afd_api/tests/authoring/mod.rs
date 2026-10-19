//! Native authoring over the full HTTP router and a real Postgres database.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "unmet integration preconditions fail loudly, and a step indexes the JSON \
              and headers it just received"
)]

mod attempt;
mod catalogue;
mod fixture;
mod input;
mod instruction_bounds;
mod library;
mod lifecycle;
mod models;
mod privacy;
mod proposals;
mod readiness;
#[path = "../../../afd_authoring/tests/support/refusal.rs"]
mod refusal;
mod runtime_role;
mod saving;
mod seed;
mod stream;

use crate::harness::{json_body, send, send_with_headers};
use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;

/// What a funded fixture wallet holds: a thousand dollars, past any reservation
/// a fixture turn or activation can make.
const FUNDED_NANOS: i64 = 1_000 * NANOS_PER_USD;

fn mint() -> Uuid7 {
    Uuid7::parse(&afd_db::test_util::mint_id()).expect("fixture UUIDv7")
}
