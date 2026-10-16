//! Compile-time source identity shared by the daemon and runner.
//!
//! The coordinated acceptance build supplies one digest to every component.
//! An ordinary unstamped build stays explicitly unbound.

/// Digest of the source files used by the coordinated build.
pub const SOURCE_ID: Option<&str> = option_env!("AGENTSFLEET_BUILD_ID");
