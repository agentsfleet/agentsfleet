//! Real ledger behavior with a scripted notification transport for failure injection.
#![cfg(feature = "test-util")]

use afd_dragonfly::test_util::fake_redis;

mod receipts;
