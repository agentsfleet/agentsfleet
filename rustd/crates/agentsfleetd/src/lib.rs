//! The daemon's own crate: what it does before it serves, and after it stops.
//!
//! A library beside the binary because a binary crate's internals cannot be
//! linked by an integration suite, and every interesting thing here is a
//! SEQUENCE — boot order, cancellation, the order pools close in — which is
//! exactly what wants testing and exactly what a `main` hides.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

// `unused_crate_dependencies` is a crate attribute, so it also grades the lib's
// own test target — and fires there for a dev-dependency only the suites in
// `tests/` import, since those are separate crates it cannot see. Naming it
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else.
#[cfg(test)]
use tracing_subscriber as _;

pub mod banner;
pub mod daemon;
pub mod fatal;
pub mod inventory;
pub mod preflight;
pub mod probes;
pub mod supervisor;
pub mod tty;

pub use self::daemon::{Daemon, Outcome, StopCause};
pub use self::inventory::{Disposition, THREAD_MAP, ThreadRow};
pub use self::preflight::{BootConfig, Fault, Refusal, preflight};
pub use self::probes::{LiveDependencies, PROBE_TIMEOUT};
pub use self::supervisor::{JOIN_TIMEOUT, ShutdownReport, Supervisor};
