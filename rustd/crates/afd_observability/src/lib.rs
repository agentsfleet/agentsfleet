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

pub mod semconv;
