//! The Postgres reads the rest of the daemon is built on.
//!
//! Named for `src/agentsfleetd/state/`, which is where the Zig daemon keeps its
//! statements. What lands here first are the three credential directories,
//! because they are the reads the authentication path cannot run without and
//! the ones `afd_auth` is deliberately unable to write for itself.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

// `unused_crate_dependencies` is a crate attribute, so it also grades the lib's
// own test target — and fires there for a dev-dependency only the suites in
// `tests/` import, since those are separate crates it cannot see. Naming it
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else.
#[cfg(test)]
use {tokio as _, tracing_subscriber as _};

pub mod credentials;
pub mod error;
pub mod sql;

pub use self::credentials::Credentials;
pub use self::error::{Result, Unavailable};
