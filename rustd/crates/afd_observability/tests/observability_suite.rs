//! Every `afd_observability` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary is a serial stretch re-paying process start. The three files
//! declared here are thirds of ONE claim — a collector that is down costs
//! telemetry, never requests — and they share a fixture shape and an assertion
//! shape, so a binary each would have bought nothing.
//!
//! Neither file is reachable on its own: `autotests = false` means cargo
//! compiles only what a `[[test]]` target names plus whatever that pulls in
//! with `#[path]`. A test file added to `tests/` and not declared here does not
//! run, does not fail, and appears in no count.

#[path = "otlp_outage_spans.rs"]
mod otlp_outage_spans;

#[path = "otlp_outage_metrics.rs"]
mod otlp_outage_metrics;

#[path = "otlp_outage_logs.rs"]
mod otlp_outage_logs;

#[path = "producers_drive.rs"]
mod producers_drive;
