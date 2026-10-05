//! The runner's telemetry: what `agentsfleet-runner run` exports, to whom, and
//! how much.
//!
//! ```text
//!   OTEL_EXPORTER_OTLP_ENDPOINT ──► Endpoint (no header, no user) ──► Telemetry
//!                                                                      │
//!   runner.lease ─► invoke_agent ─► chat / execute_tool  ──► LeaseSampler ─┤ spans
//!   turn · retry · sandbox · frames · push · tool  ──► record::* ──► Families ┤ metrics
//!                                                                      ▼
//!                                                   OTLP/HTTP ──► runner collector
//! ```
//!
//! The runner holds no observability credential. [`Endpoint`] is the only way
//! to a pipeline, and it refuses a header knob and a user in the endpoint, so
//! the credential lives with the collector on the host and nowhere a lease
//! runs. Logs stay on stderr for that collector to read from the host's log
//! store; no log pipeline is built here.
//!
//! # Recording is a free function
//!
//! The runner's families are process facts measured deep in three crates
//! (`afr_providers`, `afr_agent`, `afr_supervisor`), plus the span budget and
//! the export's own losses here. Threading
//! an instrument handle into every constructor between `main` and a retry loop
//! would put a telemetry parameter on types whose job is something else, so a
//! producer calls [`record`]'s functions and they reach whatever [`Recorder`]
//! is installed — none until `run` installs one, which makes every call a
//! no-op in `sandbox`, in `probe`, and in every test that does not ask.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

#[cfg(test)]
use tokio as _;

pub mod budget;
pub mod endpoint;
pub mod error;
pub mod families;
pub mod labels;
pub mod record;
mod telemetry;
#[cfg(feature = "test-util")]
pub mod testing;

pub use self::budget::{LeaseSampler, MAX_LEASE_SPANS, RUNNER_SPANS_PER_SECOND};
pub use self::endpoint::Endpoint;
pub use self::error::{Error, Result};
pub use self::record::Recorder;
pub use self::telemetry::{SpanLayer, Telemetry, announce_disabled};
