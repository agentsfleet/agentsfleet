//! The OTLP transport: what turns recorded telemetry into telemetry that has
//! left, for both binaries.
//!
//! Everything under `afd_observability` records into machinery that goes
//! nowhere on its own. This crate is the endpoint half, and it is its own crate
//! rather than a module of the daemon because two binaries build the same
//! pipelines from the same knobs: `agentsfleetd` with a log bridge and a
//! vendor credential, `agentsfleet-runner` with neither. A second copy in the
//! runner would be a second place for the signal paths and the counting
//! wrappers to drift (`M-SMALLER-CRATES`).
//!
//! # Three signals, three exporters, one bound
//!
//! Each signal gets its own exporter and each is wrapped in the counting
//! wrapper `afd_observability` shipped for it. That wrapper is the whole
//! failure posture: a collector that is down costs dropped batches and a
//! counter that says so, never latency on a request or a lease. Nothing here
//! is allowed to change that, which is why the transport plugs INTO the
//! wrapper rather than beside it.
//!
//! # What this crate does NOT do
//!
//! Decide policy. Which knobs a binary accepts, whether a header may be
//! configured, how faults are aggregated: those are the caller's. This crate
//! parses one knob at a time into a type that names its own refusal, and
//! builds pipelines from a configuration it has already accepted.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub mod config;
pub mod error;
pub mod pipelines;
pub mod resource;

pub use self::config::{
    DEFAULT_TIMEOUT, Encoding, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB,
    OTEL_TIMEOUT_KNOB, OtlpConfig, optional,
};
pub use self::error::{Error, Refused, Result};
pub use self::pipelines::{Builder, COLLECT_INTERVAL, Exports};
pub use self::resource::Service;
