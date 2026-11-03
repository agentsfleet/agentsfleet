//! The process inside a sandbox: take the link, confine itself, serve.
//!
//! One function, so the runner binary's `sandbox` sub-command and the kernel
//! lane's sandbox side run the same sequence.

use std::path::Path;

use afd_core::env::{ProcessEnv, log_level};
use afr_executor::Server;
use tracing::level_filters::LevelFilter;

use crate::bubblewrap::SANDBOX_WORKSPACE;
use crate::error::Result;
use crate::harden::harden;
use crate::tenant::Inherited;

/// The level the process inside logs at when the runner passes none.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Adopts the tenant leaf and the executor's link, hardens, then serves until
/// hung up.
///
/// `inherited` names the descriptors the entry inherited: every process the
/// executor starts moves into the tenant leaf, and the executor answers the
/// runner over its end of the socket pair. Both are adopted before the
/// process hardens, and the link is close-on-exec again before any process
/// starts. Call this from `main`, before anything starts a thread —
/// [`harden`] refuses once a second thread exists — and only then are
/// logging and the runtime built, neither of which starts one.
///
/// # Errors
/// A descriptor was not inherited, the process cannot be confined, the
/// runtime will not build, or the executor stops with an error.
pub fn serve_sandboxed(inherited: Inherited) -> Result<()> {
    let (tenant, link) = inherited.adopt()?;
    let server = afr_executor::connected(link)?.with_tenant(tenant);
    serve_hardened(server, Path::new(SANDBOX_WORKSPACE))
}

/// [`serve_sandboxed`] on a socket bound at `socket`, with the executor's
/// root named and no tenant leaf: its processes run where it does.
///
/// For a caller proving the sequence outside a sandbox. The root must lie under
/// one of [`crate::WRITABLE`], or the executor cannot write there once confined.
///
/// # Errors
/// The socket cannot be bound, the process cannot be confined, the runtime
/// will not build, or the executor stops with an error.
pub fn serve_confined(socket: &Path, root: &Path) -> Result<()> {
    serve_hardened(afr_executor::bind(socket)?, root)
}

/// Hardens, then serves `server` with `root` as the executor's root.
fn serve_hardened(server: Server, root: &Path) -> Result<()> {
    harden()?;
    log_to_stderr();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(server.serve(root))?;
    Ok(())
}

/// Sends this process's events to its error stream, which the supervisor
/// drains and logs, at the level the runner passed in.
fn log_to_stderr() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_max_level(log_level(&ProcessEnv, DEFAULT_LEVEL))
        .finish();
    // Set once per process; a second call, as from a test, keeps the first.
    let _already_set = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests;
