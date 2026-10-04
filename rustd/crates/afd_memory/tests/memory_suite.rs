//! Every memory integration suite, in one test binary.
//!
//! The Postgres-backed tests are marked `#[ignore]` so the unit lane compiles
//! them without a datastore; `make test-integration-rustd` runs them against
//! the real schema. A test needing no datastore runs in both lanes.

#[path = "support/paused.rs"]
mod paused;
#[path = "support/workspace.rs"]
mod workspace;

#[path = "integration_flip.rs"]
mod integration_flip;
#[path = "integration_migration.rs"]
mod integration_migration;
#[path = "integration_refusal.rs"]
mod integration_refusal;
#[path = "integration_shared.rs"]
mod integration_shared;
#[path = "integration_store.rs"]
mod integration_store;
