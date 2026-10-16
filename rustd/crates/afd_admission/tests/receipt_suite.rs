//! What an admission does once its row is committed and the queue, the ledger
//! or the readiness index then answers wrongly.
//!
//! The row is the record: every case below still answers the producer with its
//! fresh event, and the difference is what the ledger and the log say
//! afterwards. The queue is a fake — a real Dragonfly never runs out of memory
//! or refuses a hash write on demand — and Postgres is the lane's.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "../../afd_dragonfly/tests/support/subscriber.rs"]
mod subscriber;

#[path = "../../afd_dragonfly/tests/support/fake_redis/mod.rs"]
#[allow(
    unused_imports,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::format_push_string,
    reason = "test support shared with `afd_dragonfly`, which uses the half this suite does not"
)]
mod fake_redis;

mod receipts;
