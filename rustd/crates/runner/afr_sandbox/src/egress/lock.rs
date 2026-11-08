//! One process at a time owns a host's egress objects.
//!
//! The boot sweep removes every `afegress*` table and `afv*` link this process
//! does not hold, and slot claims are counted per process ([`super::slot`]). A
//! second runner process on the same host — `agentsfleet-runner run` started
//! by hand beside the service, or the kernel lane on a serving host — would
//! delete the first one's live links and then build over its slots. The engine
//! takes this lock before its sweep and keeps it for the life of the process,
//! so the second process refuses at boot instead.
//!
//! The lock is an advisory `flock` on a file in the runtime directory the unit
//! gives the runner (`RuntimeDirectory=agentsfleet`). No sandbox sees that
//! directory, even one sharing the host's network, so a tenant cannot take the
//! lock first; the kernel drops it when the process ends, however it ends.

use std::fs::{self, File, TryLockError};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use crate::error::{EgressRefusal, Error, Result, egress_refused};

/// The runner's runtime directory on the host, and the lock file in it.
const RUNTIME_DIR: &str = "/run/agentsfleet";
const LOCK_FILE: &str = "egress.lock";

/// The lock this process holds, once taken. Every engine the process builds
/// shares it, as they share the slot claims.
static HELD: Mutex<Option<File>> = Mutex::new(None);

/// Takes the host's egress lock for this process, or confirms it holds it.
///
/// # Errors
/// Another process holds the lock, or the lock file cannot be opened.
pub(crate) fn own_host() -> Result<()> {
    let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_none() {
        *held = Some(take(Path::new(RUNTIME_DIR))?);
    }
    Ok(())
}

/// Opens the lock file in `dir`, making both when absent, and locks it for
/// this open file alone, without waiting.
fn take(dir: &Path) -> Result<File> {
    fs::create_dir_all(dir)?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_FILE))?;
    file.try_lock().map_err(refused)?;
    Ok(file)
}

/// Why the lock was not taken: another process holds it, or the call itself
/// failed, which is that failure and never a holder.
fn refused(failed: TryLockError) -> Error {
    match failed {
        TryLockError::WouldBlock => egress_refused(EgressRefusal::HeldElsewhere),
        TryLockError::Error(error) => error.into(),
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
