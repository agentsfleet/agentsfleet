//! Every `afd_approval` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. The support module is declared once and reached as
//! `crate::lane`.
//!
//! # Aggregation is not what made this suite safe
//!
//! The audit that preceded it grepped the suite files for `total()`, `COUNT(`
//! and unfiltered listings, found none, and cleared the crate. It was looking
//! in the wrong place: the count lives in `lane::event_count`, a support
//! helper, and the tests that call it read as `lane.event_count().await`. The
//! suite went red on it three runs running.
//!
//! What makes it safe is `docs/architecture/testing.md` rules ISO-1 to ISO-3, and those
//! hold at any binary count: every test mints its own workspace and fleet, and
//! the tests that meet the global sweeper take `lane::sweeper_exclusive`. Both
//! tests in one file already ran concurrently, so neither this file nor its
//! absence ever changed that.

#[path = "support/gate_lane.rs"]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
mod lane;

#[path = "integration_grants.rs"]
mod integration_grants;
#[path = "integration_inbox.rs"]
mod integration_inbox;
