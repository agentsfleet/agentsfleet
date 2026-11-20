//! guffbolt driven the way a third party drives it: through its public API
//! alone, over a scripted model, with no `crate::` internal in reach.
//!
//! One binary for every file here, as the workspace keeps its test binaries
//! few: cargo runs binaries one after another, and none of these needs a
//! datastore. The files that build tools through `Tools::builder()` need the
//! `tools` feature; the plain loop runs without it.

#![expect(
    clippy::expect_used,
    reason = "test binary: a failed precondition should fail the test loudly"
)]

mod plain;
mod script;

#[cfg(feature = "tools")]
mod children;
#[cfg(feature = "tools")]
mod family;
#[cfg(feature = "tools")]
mod files;
#[cfg(feature = "tools")]
mod network;
#[cfg(feature = "tools")]
mod process;
#[cfg(feature = "tools")]
mod resume;
#[cfg(feature = "tools")]
mod schedule;
#[cfg(feature = "tools")]
mod small_tools;
#[cfg(feature = "tools")]
mod tools_loop;
