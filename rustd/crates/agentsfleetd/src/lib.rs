//! The daemon's own crate: what it does before it serves, and after it stops.
//!
//! A library beside the binary because a binary crate's internals cannot be
//! linked by an integration suite, and every interesting thing here is a
//! SEQUENCE — boot order, cancellation, the order pools close in — which is
//! exactly what wants testing and exactly what a `main` hides.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub mod banner;
pub mod cli;
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
pub mod tty;

pub use self::cli::{Cli, Command};
pub use self::daemon::{Daemon, Outcome, StopCause};
pub use self::error::{BootFailure, Fault, MigrateFailure, Refusal};
pub use self::identity::{Capabilities, Sessions};
pub use self::inventory::{BACKGROUND_TASKS, HUB_PUMP, OTLP_EXPORT};
pub use self::migrate::migrate;
pub use self::plane::{Authenticator, ServingPlane, Shared};
pub use self::preflight::{BootConfig, IdentityConfig, preflight};
pub use self::probes::{LiveDependencies, PROBE_TIMEOUT};
pub use self::supervisor::{JOIN_TIMEOUT, ShutdownReport, Supervisor};
