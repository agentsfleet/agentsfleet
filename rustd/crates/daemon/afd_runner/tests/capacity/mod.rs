//! Real Postgres host capacity: live runs fill its workers, idle parks take none.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

mod choice;
mod concurrent;
mod expiry;
mod fixture;
mod parks;

use self::fixture::Fixture;
