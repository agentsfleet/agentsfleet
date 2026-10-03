//! Every Postgres-backed memory suite, in one test binary.
//!
//! Marked `#[ignore]` so the unit lane compiles them without a datastore;
//! `make test-integration-rustd` runs them against the real schema.

#[path = "support/workspace.rs"]
mod workspace;

#[path = "integration_migration.rs"]
mod integration_migration;
#[path = "integration_shared.rs"]
mod integration_shared;
#[path = "integration_store.rs"]
mod integration_store;
