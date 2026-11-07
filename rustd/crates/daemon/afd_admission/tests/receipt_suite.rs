//! Real ledger behavior with a scripted notification transport for failure injection.
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
