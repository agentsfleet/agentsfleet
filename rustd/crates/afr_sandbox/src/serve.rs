//! The process inside a sandbox: confine itself, then serve the executor.
//!
//! One function, so the runner binary's `sandbox` sub-command and the kernel
//! lane's sandbox side run the same sequence.

use std::path::Path;

use crate::bubblewrap::{SANDBOX_SOCKET, SANDBOX_WORKSPACE};
use crate::error::Result;
use crate::harden::harden;

/// Hardens the calling process, then serves the executor on the sandbox's
/// socket until the supervisor hangs up.
///
/// Call it first, from `main`, before anything starts a thread: [`harden`]
/// refuses once a second thread exists, and the executor's runtime is built
/// only after it has succeeded.
///
/// # Errors
/// The process cannot be confined, the runtime will not build, or the
/// executor stops with an error.
pub fn serve_sandboxed() -> Result<()> {
    harden()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(afr_executor::serve(
        Path::new(SANDBOX_SOCKET),
        Path::new(SANDBOX_WORKSPACE),
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests;
