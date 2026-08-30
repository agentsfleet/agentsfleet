//! Every `agentsfleetd` test file, in one test binary.
//!
//! Sixteen binaries became one. cargo runs test BINARIES serially and the tests
//! inside one binary in parallel, so each was a serial stretch that re-paid
//! process start, dynamic linking and its own datastore connections.
//!
//! # Why this crate is safe to aggregate
//!
//! Aggregation makes a crate's suites run CONCURRENTLY against whatever they
//! share, which is what broke `afd_fleet` — a test there asserted a global row
//! count held still across a paginated walk, and a sibling suite moved it. The
//! e2e scenarios here DO share the lane's database: `scenario_database` returns
//! the lane URL unchanged, and `scenario` says so, keeping runs apart by the
//! identifiers `unique_ids` mints rather than by a database apiece. That design
//! survives concurrency, and nothing here asserts a global count — the audit
//! that preceded this change found no `total()`, `COUNT(`, or bare-length
//! assertion over a shared table. Every bound port is already `EPHEMERAL`.
//!
//! # The support modules are declared here, once
//!
//! Three files each re-declared the same five e2e helpers, and seven more each
//! re-declared `support`. They are declared once below and reached as
//! `crate::<name>` — which is the spelling those helpers ALREADY used among
//! themselves, because a test binary's root crate is what `crate::` meant when
//! each file was its own binary. The aggregator is now that root.
//!
//! `money_gate` is deliberately NOT hoisted: it stays declared inside
//! `integration_runner_e2e` with its original `#[path]`, because it opens with
//! `use super::*` and that path resolves against the directory of the file that
//! declares it — which aggregation does not move.

#[path = "support/e2e.rs"]
mod e2e;
#[path = "support/e2e_db.rs"]
mod e2e_db;
#[path = "support/e2e_seed.rs"]
mod e2e_seed;
#[path = "support/e2e_reads.rs"]
mod reads;
#[path = "support/mod.rs"]
mod support;
#[path = "support/e2e_wire.rs"]
mod wire;

#[path = "binary.rs"]
mod binary;
#[path = "cli.rs"]
mod cli;
#[path = "daemon.rs"]
mod daemon;
#[path = "integration_cli.rs"]
mod integration_cli;
#[path = "integration_readyz.rs"]
mod integration_readyz;
#[path = "integration_runner_activity.rs"]
mod integration_runner_activity;
#[path = "integration_runner_e2e.rs"]
mod integration_runner_e2e;
#[path = "integration_runner_shapes.rs"]
mod integration_runner_shapes;
#[path = "integration_serve.rs"]
mod integration_serve;
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
#[path = "signal.rs"]
mod signal;
#[path = "supervisor.rs"]
mod supervisor;
