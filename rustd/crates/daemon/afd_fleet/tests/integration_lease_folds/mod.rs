//! A live run claims what its fleet receives while it works, and its report
//! settles what it read, against live Postgres.
//!
//! The claim is one statement and the settle three more, so these drive the
//! lease store rather than the whole plane: what they prove is which rows the
//! statements touch, in which order, under which fence.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without
//! datastores; `make test-integration-rustd` is the only lane that runs them.
#![cfg(feature = "test-util")]

mod claims;
mod conversations;
mod limits;
mod settles;
mod support;
mod waiting;
