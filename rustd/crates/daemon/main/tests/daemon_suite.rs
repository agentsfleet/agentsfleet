//! Every `agentsfleetd` test file that boots the daemon alone, in one test
//! binary.
//!
//! cargo runs test BINARIES serially and the tests inside one binary in
//! parallel, so one binary is one serial stretch instead of many that each
//! re-pay process start, dynamic linking and their datastore connections.
//!
//! The suites that drive a runner against this daemon are `af_e2e`'s, so a
//! daemon test build compiles no runner crate. The knobs and subscriber both
//! share come from `agentsfleetd::test_util`.

#[path = "binary.rs"]
mod binary;
#[path = "cli/mod.rs"]
mod cli;
#[path = "daemon.rs"]
mod daemon;
#[path = "integration_cli/mod.rs"]
mod integration_cli;
#[path = "integration_readyz.rs"]
mod integration_readyz;
#[path = "integration_serve.rs"]
mod integration_serve;
#[path = "integration_telemetry.rs"]
mod integration_telemetry;
#[path = "migrate.rs"]
mod migrate;
#[path = "nameplate.rs"]
mod nameplate;
#[path = "preflight.rs"]
mod preflight;
#[path = "preflight_optional.rs"]
mod preflight_optional;
#[path = "presentation.rs"]
mod presentation;
#[path = "serve.rs"]
mod serve;
mod shutdown_drain;
#[path = "supervisor.rs"]
mod supervisor;
#[path = "tls_posture.rs"]
mod tls_posture;
