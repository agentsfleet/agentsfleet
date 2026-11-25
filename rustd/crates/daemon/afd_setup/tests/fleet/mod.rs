//! Every database proof owns its people, workspace, models and fleets.
//!
//! Marked `#[ignore]` so the unit lane compiles them without datastores;
//! `make test-integration-rustd` runs them.

#![expect(
    clippy::expect_used,
    reason = "fixture preconditions fail the proof loudly"
)]

mod create;
mod delete;
mod fixture;
mod patch;
mod seed;

use af_core::id::Uuid7;

fn mint() -> Uuid7 {
    afd_db::test_util::mint_id()
}
