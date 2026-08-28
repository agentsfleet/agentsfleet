//! Push-only telemetry: what this daemon says about itself, and to whom.
//!
//! # Why this crate knows nothing about HTTP
//!
//! It carries the attribute vocabulary and the export machinery, and it is
//! deliberately free of any web framework. The layer that puts a route template
//! on a span lives with the router, in `afd_api`, because that is where a route
//! template is a fact; here it is a string somebody chose. Keeping the
//! dependency pointing that way also leaves this crate usable by the runner
//! binary, which serves no HTTP at all.
//!
//! # What is not here yet
//!
//! The OTLP transport. It is constructed from configuration at boot, which is
//! §7's, and an exporter this crate built for nobody would be a dependency and
//! a code path with no caller. What IS here is everything the transport plugs
//! into — so the piece boot adds is the endpoint, not the design.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

// `unused_crate_dependencies` is a crate attribute, so it also grades the lib's
// own test target — and fires there for a dev-dependency only the suites in
// `tests/` import, since those are separate crates it cannot see. Naming them
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else.
#[cfg(test)]
use {opentelemetry as _, tokio as _};

pub mod export;
pub mod product;
pub mod runner;
pub mod semconv;

pub use self::export::{CountingExporter, SpanDrops};
pub use self::product::{Analytics, Telemetry};
