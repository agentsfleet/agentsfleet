//! The host-local storage root: what lives under it, and the boot sweep.
//!
//! ```text
//!   <home>/leases/<lease_id>/   one lease's scratch; gone when the lease ends
//!   <home>/spool/<lease_id>.json  a report written before it is posted
//!   <home>/bundles/<hash>.tar   fleet bundles, verified before they are kept
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use afd_core::id::Uuid7;

use crate::error::Result;

const LEASES: &str = "leases";
const SPOOL: &str = "spool";
const BUNDLES: &str = "bundles";

/// The storage root, with its three directories made.
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
        for directory in [home.leases(), home.spool(), home.bundles()] {
            fs::create_dir_all(directory)?;
        }
        Ok(home)
    }

    /// Where one lease's scratch lives.
    #[must_use]
    pub fn lease_dir(&self, lease_id: &Uuid7) -> PathBuf {
        self.leases().join(lease_id.as_str())
    }

    /// Where reports wait to be posted.
    #[must_use]
    pub fn spool(&self) -> PathBuf {
        self.root.join(SPOOL)
    }

    /// Where verified bundles are kept.
    #[must_use]
    pub fn bundles(&self) -> PathBuf {
        self.root.join(BUNDLES)
    }

    fn leases(&self) -> PathBuf {
        self.root.join(LEASES)
    }

    /// Removes every lease directory a previous process left behind.
    ///
    /// Runs at boot, when no lease is in flight, so every directory named like
    /// a lease is an orphan. Anything else under the root — a file an operator
    /// put there, a directory not named like a lease — is not this runner's to
    /// remove, and stays. Returns how many directories went.
    ///
    /// # Errors
    /// A directory that cannot be listed or removed.
    pub fn sweep(&self) -> Result<usize> {
        let mut swept = 0;
        for entry in fs::read_dir(self.leases())? {
            let path = entry?.path();
            if is_lease_dir(&path) {
                fs::remove_dir_all(&path)?;
                swept += 1;
            }
        }
        Ok(swept)
    }
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
