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
// `tests/` import, since those are separate crates it cannot see. Naming them
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else. All three belong to §7's end-to-end suite.
#[cfg(test)]
use {afd_wire as _, serde_json as _, sqlx as _};

pub mod banner;
pub mod bundles;
pub mod cli;
pub mod credentials;
pub mod daemon;
pub mod error;
pub mod fatal;
pub mod identity;
pub mod inventory;
pub mod logs;
pub mod migrate;
pub mod plane;
pub mod preflight;
pub mod probes;
pub mod serve;
pub mod signal;
pub mod supervisor;
pub mod sweepers;
pub mod tty;

pub use self::cli::{Cli, Command};
pub use self::daemon::{Daemon, Outcome, StopCause};
pub use self::error::{BootFailure, Fault, MigrateFailure, Refusal};
pub use self::identity::{Capabilities, Sessions};
pub use self::inventory::{BACKGROUND_TASKS, HUB_PUMP, OTLP_EXPORT};
pub use self::migrate::migrate;
pub use self::plane::{Authenticator, ServingPlane, Shared};
pub use self::preflight::{BootConfig, BundleStoreConfig, IdentityConfig, preflight};
pub use self::probes::{LiveDependencies, PROBE_TIMEOUT};
pub use self::supervisor::{JOIN_TIMEOUT, ShutdownReport, Supervisor};
