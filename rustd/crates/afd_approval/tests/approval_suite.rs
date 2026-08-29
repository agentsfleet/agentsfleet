//! Every `afd_approval` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. The audit preceding every aggregation on this branch
//! ran here too: no suite asserts over global state — no `total()`, `COUNT(`
//! or unfiltered listing over a shared table — so concurrency between these
//! suites has nothing to race on. The support module is declared once and
//! reached as `crate::lane`.

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
