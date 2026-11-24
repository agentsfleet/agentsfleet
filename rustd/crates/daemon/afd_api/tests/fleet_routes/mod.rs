//! A fleet's routes over the full HTTP router and a real Postgres database:
//! the workspace's model choices, a create from a first message, a read by
//! any member, a person's network toggle, the access a person approves and
//! takes back, and the triggers a person sees and removes.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "unmet integration preconditions fail loudly, and a step indexes the JSON \
              and headers it just received"
)]

mod access;
mod fixture;
mod fleets;
mod models;
mod seed;
mod triggers;

use crate::harness::{json_body, send, send_with_headers};
use af_core::id::Uuid7;

fn mint() -> Uuid7 {
    afd_db::test_util::mint_id()
}
