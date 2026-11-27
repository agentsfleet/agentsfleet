//! A process confining itself, then serving the guest agent on a bound socket:
//! the sequence [`crate::harden`] is proven through outside a sandbox.

use std::path::Path;

use af_core::env::{ProcessEnv, log_level};
use afg_agent::Server;
use tracing::level_filters::LevelFilter;

use crate::error::Result;
use crate::harden::harden;

/// The level the process logs at when the caller passes none.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Binds a socket at `socket`, hardens, then serves the guest agent with `root`
/// as its root, its processes running where it does.
///
/// For a caller proving the sequence outside a sandbox. The root must lie under
/// one of [`crate::WRITABLE`], or the guest agent cannot write there once confined.
/// Call this before anything starts a thread — [`harden`] refuses once a
/// second thread exists — and only then are logging and the runtime built,
/// neither of which starts one.
///
/// # Errors
/// The socket cannot be bound, the process cannot be confined, the runtime
/// will not build, or the guest agent stops with an error.
pub fn serve_confined(socket: &Path, root: &Path) -> Result<()> {
    serve_hardened(afg_agent::bind(socket)?, root)
}

/// Hardens, then serves `server` with `root` as the guest agent's root.
fn serve_hardened(server: Server, root: &Path) -> Result<()> {
    harden()?;
    log_to_stderr();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(server.serve(root))?;
    Ok(())
}

/// Sends this process's events to its error stream, at the level its
/// environment names.
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
