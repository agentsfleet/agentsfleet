//! The cluster-rebuild proof, alone in its own test binary.
//!
//! Cargo runs test BINARIES serially and the tests inside one binary in
//! parallel, which is why `fleet_suite.rs` aggregates every other suite into
//! one. This proof is the exception that earns a second binary: it marks
//! `BATCH_LIMIT + 1` fleets ready at once — the population its wrapping-cursor
//! assertion needs — and the readiness index those marks land in is process-
//! global. Run beside the other suites it takes their polls down with it, one
//! different neighbour per run; run alone it is green in about four seconds.
//!
//! The mechanism behind that contention is NOT yet understood. Two candidates
//! were measured and rejected: the bounded peek (`MAX_READY_CANDIDATES_PER_POLL`
//! is 64, and 101 marks over 16 partitions average seven) and stale marks left
//! by staging (clearing each one as its fleet was leased changed nothing). So
//! this file is containment with its reason stated, not a diagnosis — whatever
//! breaks a staging poll under load is still there, and a future suite that
//! polls under the same pressure can still meet it.

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;
#[path = "support/fleet_queue.rs"]
mod queue;
#[path = "support/fleet_report_reads.rs"]
mod report_reads;
#[path = "support/fleet_report_seed.rs"]
mod report_seed;
#[path = "support/fleet_requests.rs"]
mod requests;
#[path = "support/fleet_lease_seed.rs"]
mod seed;
#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "integration_cluster_rebuild.rs"]
mod integration_cluster_rebuild;
