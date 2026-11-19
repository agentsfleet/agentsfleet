//! Real Postgres host capacity, shared between authoring and runtime runs.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

mod choice;
mod concurrent;
mod expiry;
mod fixture;
mod holds;

use self::fixture::Fixture;
