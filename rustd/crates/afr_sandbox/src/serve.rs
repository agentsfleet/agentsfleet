//! The process inside a sandbox: take the socket, confine itself, serve.
//!
//! One function, so the runner binary's `sandbox` sub-command and the kernel
//! lane's sandbox side run the same sequence.

use std::path::Path;

use afd_core::env::{ProcessEnv, log_level};
use tracing::level_filters::LevelFilter;

use crate::bubblewrap::{SANDBOX_WORKSPACE, sandbox_socket};
use crate::error::Result;
use crate::harden::harden;

/// The level the process inside logs at when the runner passes none.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Binds the executor's socket, hardens the calling process, then serves the
/// executor until the supervisor hangs up.
///
/// The socket is bound first, while its directory is still writable: once
/// hardened, nothing in the sandbox may write there again. Call this from
/// `main`, before anything starts a thread — [`harden`] refuses once a second
/// thread exists — and only then are logging and the runtime built, neither
/// of which starts one.
///
/// # Errors
/// The socket cannot be bound, the process cannot be confined, the runtime
/// will not build, or the executor stops with an error.
pub fn serve_sandboxed() -> Result<()> {
    serve_confined(&sandbox_socket(), Path::new(SANDBOX_WORKSPACE))
}

/// [`serve_sandboxed`] with the socket and the executor's root named.
///
/// For a caller proving the sequence outside a sandbox. The root must lie under
/// one of [`crate::WRITABLE`], or the executor cannot write there once confined.
///
/// # Errors
/// As [`serve_sandboxed`].
pub fn serve_confined(socket: &Path, root: &Path) -> Result<()> {
    let listener = afr_executor::bind(socket)?;
    harden()?;
    log_to_stderr();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(listener.serve(root))?;
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
