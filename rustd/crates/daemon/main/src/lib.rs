//! The daemon's own crate: what it does before it serves, and after it stops.
//!
//! A library beside the binary because a binary crate's internals cannot be
//! linked by an integration suite, and every interesting thing here is a
//! SEQUENCE — boot order, cancellation, the order pools close in — which is
//! exactly what wants testing and exactly what a `main` hides.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

// `unused_crate_dependencies` is a crate attribute, so it also grades the lib's
// own test target — and fires there for a dev-dependency only the suites in
// `tests/` import, since those are separate crates it cannot see. Naming them
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else: the TLS posture proof's two crates, and the bodies and
// scratch directories the boot suites build.
#[cfg(test)]
use {rcgen as _, rustls as _, serde_json as _, tempfile as _};

pub mod banner;
pub mod billing;
pub mod cli;
pub mod config;
pub mod credentials;
pub mod daemon;
pub mod error;
pub mod fatal;
pub mod identity;
pub mod inventory;
pub mod logs;
pub mod migrate;
pub mod nameplate;
pub mod outbound;
pub mod plane;
pub mod preflight;
pub mod probes;
pub mod serve;
pub mod supervisor;
pub mod sweepers;
pub mod telemetry;
#[cfg(feature = "test-util")]
pub mod test_util;
pub mod tty;

pub use self::cli::{Cli, Command};
pub use self::daemon::{Daemon, Outcome, StopCause};
pub use self::error::{BootFailure, Fault, MigrateFailure, Refusal};
pub use self::identity::{Capabilities, Sessions};
pub use self::inventory::{
    ANALYTICS_FLUSH, BACKGROUND_TASKS, BUS_SHUTDOWN, OTLP_EXPORT, OUTBOUND_WORKER,
};
pub use self::migrate::migrate;
pub use self::plane::{Authenticator, ServingPlane, Shared};
pub use self::preflight::{BootConfig, IdentityConfig, preflight};
pub use self::probes::{LiveDependencies, PROBE_TIMEOUT};
pub use self::supervisor::{JOIN_TIMEOUT, ShutdownReport, Supervisor};
