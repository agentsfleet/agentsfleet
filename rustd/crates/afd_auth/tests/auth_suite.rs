//! Every `afd_auth` test file, in one test binary.
//!
//! One binary rather than 4: cargo runs test BINARIES serially and the tests
//! inside one binary in parallel, so each extra binary bought a serial stretch
//! and re-paid its own process start and dynamic linking.
//!
//! Safe to aggregate because these suites share no datastore, and touch no live Postgres or Redis at all. That is
//! the check aggregation actually turns on, and it is not a formality: doing
//! this to `afd_fleet` made eighteen suites concurrent against one Postgres and
//! broke a test asserting a global row count held still across a paginated
//! walk. Crates whose suites take `TestDatabase::shared` — `afd_runner` and
//! `afd_tenant` — are deliberately NOT aggregated for that reason.

#[path = "auth_error_taxonomy.rs"]
mod auth_error_taxonomy;
#[path = "bearer_prefix_routing.rs"]
mod bearer_prefix_routing;
#[path = "gate.rs"]
mod gate;
#[path = "scope_catalogue.rs"]
mod scope_catalogue;
