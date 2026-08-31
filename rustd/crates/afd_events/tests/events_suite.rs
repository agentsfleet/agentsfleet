//! Every `afd_events` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. The audit preceding every aggregation on this branch
//! ran here too: no suite asserts over global state — no `total()`, `COUNT(`
//! or unfiltered listing over a shared table — so concurrency between these
//! suites has nothing to race on. The support module is declared once and
//! reached as `crate::support`.

#[path = "support/events_lane.rs"]
mod support;

// No datastore: a source scan over the workspace, so it carries no `#[ignore]`
// and runs in the unit lane (`docs/architecture/testing.md`, filename rule).
#[path = "fleet_event_writers.rs"]
mod fleet_event_writers;

#[path = "integration_backfill.rs"]
mod integration_backfill;
#[path = "integration_steer.rs"]
mod integration_steer;
