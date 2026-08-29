//! Every `afd_vault` test file, in one test binary.
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

#[path = "integration_list_no_decrypt.rs"]
mod integration_list_no_decrypt;
#[path = "integration_projection_parity.rs"]
mod integration_projection_parity;
#[path = "integration_reference_lock.rs"]
mod integration_reference_lock;
