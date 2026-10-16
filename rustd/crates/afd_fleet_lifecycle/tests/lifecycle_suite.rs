//! Every `afd_fleet_lifecycle` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary was a serial stretch re-paying process start and its own
//! datastore connections. No suite asserts over global state — no `total()`,
//! `COUNT(` or unfiltered listing over a shared table — so concurrency between
//! these suites has nothing to race on. The support module is declared once
//! and reached as `crate::support`.

mod support;

mod integration_fleet_page;
mod integration_purge_ledger_identity;
mod integration_purge_privileges;
mod integration_wall_counters;
