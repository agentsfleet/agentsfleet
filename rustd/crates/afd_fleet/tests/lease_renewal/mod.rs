//! Original-expiry checks use database time after the shared host lock.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions must fail loudly"
)]

mod expiry;
mod fixture;
mod holders;
mod lock_wait;
mod mixed;
mod replica;
mod replicas;
mod seat;
mod wallet;
