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

use crate::error::{Result, egress_refused};

/// The lock file, in the runner's runtime directory on the host.
const HOST_LOCK: &str = "/run/agentsfleet/egress.lock";
/// Why a second process may not own the host's egress.
const HELD_ELSEWHERE: &str = "another runner process owns this host's egress tables and links";

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
        *held = Some(take(Path::new(HOST_LOCK))?);
    }
    Ok(())
}

/// Opens `path`, making it and its directory when absent, and locks it for
/// this open file alone, without waiting.
fn take(path: &Path) -> Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(egress_refused(HELD_ELSEWHERE)),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "lock_tests.rs"]
mod tests;
