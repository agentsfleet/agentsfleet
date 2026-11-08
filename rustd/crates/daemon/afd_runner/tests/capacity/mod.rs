//! Real Postgres host capacity, shared between authoring and runtime leases.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

mod choice;
mod concurrent;
mod expiry;
mod fixture;

use self::fixture::Fixture;
