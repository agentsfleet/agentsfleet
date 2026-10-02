//! The host-local storage root: what lives under it, and the boot sweep.
//!
//! ```text
//!   <home>/sandboxes/<lease_id>/  each lease's sandbox; the engine's base
//!   <home>/spool/<lease_id>.json  a report written before it is posted
//!   <home>/bundles/<hash>.tar     fleet bundles, verified before they are kept
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use afd_core::error_code;
use afd_core::id::Uuid7;

use crate::error::Result;

const SANDBOXES: &str = "sandboxes";
const SPOOL: &str = "spool";
const BUNDLES: &str = "bundles";
const EVENT_SWEPT: &str = "storage_home_swept";
const EVENT_SWEEP_FAILED: &str = "storage_home_sweep_failed";

/// The storage root, with its directories made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageHome {
    root: PathBuf,
}

impl StorageHome {
    /// Opens `root`, making it and its directories when they are missing.
    ///
    /// # Errors
    /// A directory that cannot be made.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let home = Self { root: root.into() };
        for directory in [home.sandboxes(), home.spool(), home.bundles()] {
            fs::create_dir_all(directory)?;
        }
        Ok(home)
    }

    /// Where lease sandboxes live: the base every engine is built with, so the
    /// boot sweep finds what a crashed runner left there.
    #[must_use]
    pub fn sandboxes(&self) -> PathBuf {
        self.root.join(SANDBOXES)
    }

    /// Where reports wait to be posted.
    pub(crate) fn spool(&self) -> PathBuf {
        self.root.join(SPOOL)
    }

    /// Where verified bundles are kept.
    pub(crate) fn bundles(&self) -> PathBuf {
        self.root.join(BUNDLES)
    }

    /// Removes every lease sandbox a previous process left behind, and returns
    /// how many went.
    ///
    /// Runs at boot, when no lease is in flight, so every directory named like
    /// a lease is an orphan. Anything else — a warm slot, a file an operator
    /// put there — is not this runner's to remove, and stays. The sweep never
    /// refuses boot: a directory that will not go (still mounted, or not this
    /// process's to delete) is logged and left.
    #[must_use]
    pub fn sweep(&self) -> usize {
        self.sweep_with(|orphan| fs::remove_dir_all(orphan))
    }

    pub(crate) fn sweep_with(&self, remove: impl Fn(&Path) -> io::Result<()>) -> usize {
        let base = self.sandboxes();
        let orphans = match fs::read_dir(&base) {
            Ok(entries) => entries.filter_map(|entry| entry.ok().map(|entry| entry.path())),
            Err(failure) => {
                unswept(&base, &failure);
                return 0;
            }
        };
        let swept = orphans
            .filter(|path| is_lease_dir(path))
            .filter(|orphan| {
                remove(orphan)
                    .inspect_err(|failure| unswept(orphan, failure))
                    .is_ok()
            })
            .count();
        let event = EVENT_SWEPT;
        tracing::info!(swept, event);
        swept
    }
}

/// Logs a directory the sweep could not remove.
fn unswept(path: &Path, failure: &io::Error) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let path = path.display().to_string();
    let reason = failure.to_string();
    let event = EVENT_SWEEP_FAILED;
    tracing::warn!(
        error_code = code,
        path,
        reason,
        event,
        "an orphan stays until it can go"
    );
}

/// Whether `path` is a directory named by a lease identifier.
fn is_lease_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| Uuid7::parse(name).is_ok())
}

#[cfg(test)]
#[path = "storage_home/tests.rs"]
mod tests;
