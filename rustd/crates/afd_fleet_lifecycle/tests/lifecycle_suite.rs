//! Every `afd_fleet_lifecycle` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. The audit preceding every aggregation on this branch
//! ran here too: no suite asserts over global state — no `total()`, `COUNT(`
//! or unfiltered listing over a shared table — so concurrency between these
//! suites has nothing to race on. The support module is declared once and
//! reached as `crate::support`.

#[path = "support/lane.rs"]
mod support;

#[path = "integration_install_rollback.rs"]
mod integration_install_rollback;
#[path = "integration_patch_visibility.rs"]
mod integration_patch_visibility;
