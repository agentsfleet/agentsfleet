//! The suites that drive both binaries at once, and the rule that keeps every
//! other crate in its layer.
//!
//! `e2e/` is the one layer folder that may depend on every other layer, and
//! nothing depends on it. That is what lets one crate boot `agentsfleetd` and
//! drive the Rust runner against it while neither binary's crate names the
//! other's, in any dependency table (`rustd/Cargo.toml`).
//!
//! The library holds only these docs. The targets are the work:
//!
//! - `tests/e2e_suite.rs`: the daemon-and-runner suites, `#[ignore]`d, run by
//!   `make test-integration-rustd` against live Postgres and NATS;
//! - `tests/integration_verb_span_export.rs`: a verb's spans through the
//!   installed exporter, in a binary of its own because it owns the tracer;
//! - `tests/layers.rs`: the layer rule, read from the manifests, with no
//!   datastore, so it runs in `make test-unit-all`;
//! - `benches/recovery`: the recovery burst through both processes.
