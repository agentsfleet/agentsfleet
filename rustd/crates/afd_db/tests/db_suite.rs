//! Every `afd_db` test file, in one test binary.
//!
//! One binary rather than nine, for the reason the other suites record: cargo
//! runs test binaries serially and the tests inside one in parallel, so each
//! extra binary bought a serial stretch and re-paid its own process start.
//!
//! # Why this crate is safe to aggregate and two others are not
//!
//! Aggregation makes a crate's suites run CONCURRENTLY against whatever they
//! share. Every scratch-database test here takes `TestDatabase::create`, which
//! is a database per test — eighteen call sites, no `shared()` — so there is
//! nothing for them to race over. `afd_runner` and `afd_tenant` take
//! `TestDatabase::shared` instead and are deliberately left alone; `afd_fleet`
//! is what taught us the difference, by breaking a test that asserted a global
//! row count held still while a sibling suite enrolled a runner.
//!
//! # The nested modules are NOT re-declared here
//!
//! `integration_migrate` and `integration_migrate_faults` load their own
//! children through `#[path]`, and those paths resolve against the directory of
//! the file that DECLARES them — which aggregation does not move. Hoisting them
//! up here would both duplicate the modules and break `super::` inside them.

#[path = "config.rs"]
mod config;
#[path = "error_surface.rs"]
mod error_surface;
#[path = "integration_migrate.rs"]
mod integration_migrate;
#[path = "integration_migrate_faults.rs"]
mod integration_migrate_faults;
#[path = "integration_pool.rs"]
mod integration_pool;
#[path = "integration_pool_faults.rs"]
mod integration_pool_faults;
#[path = "lock_policy.rs"]
mod lock_policy;
#[path = "migrations.rs"]
mod migrations;
#[path = "sql_statements.rs"]
mod sql_statements;
