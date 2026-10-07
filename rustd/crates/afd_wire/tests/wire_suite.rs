//! Every `afd_wire` test file, in one test binary.
//!
//! One binary rather than 9: cargo runs test BINARIES serially and the tests
//! inside one binary in parallel, so each extra binary bought a serial stretch
//! and re-paid its own process start and dynamic linking.
//!
//! Safe to aggregate because these suites share no datastore, and touch no live Postgres or Dragonfly at all. That is
//! the check aggregation actually turns on, and it is not a formality: doing
//! this to `afd_fleet` made eighteen suites concurrent against one Postgres and
//! broke a test asserting a global row count held still across a paginated
//! walk. Crates whose suites take `TestDatabase::shared` — `afd_runner` and
//! `afd_tenant` — are deliberately NOT aggregated for that reason.

#[path = "memory_shapes.rs"]
mod memory_shapes;
#[path = "policy_shapes.rs"]
mod policy_shapes;
#[path = "redaction.rs"]
mod redaction;
// Every runner route template, pinned in one reviewed snapshot.
#[path = "routes.rs"]
mod routes;
#[path = "strictness.rs"]
mod strictness;
// Declared bounds at their exact limits, the steer request's own rows, and a
// seeded mutation corpus the parser must survive without panicking.
#[path = "validation.rs"]
mod validation;
#[path = "validation_holds.rs"]
mod validation_holds;
#[path = "validation_lease.rs"]
mod validation_lease;
#[path = "validation_mutation.rs"]
mod validation_mutation;
#[path = "validation_steer.rs"]
mod validation_steer;
