//! The host-local storage root, and what lives under it.
//!
//! ```text
//!   <home>/sandboxes/<lease_id>/  each lease's sandbox; the engine's base
//!   <home>/spool/<lease_id>.json  a report written before it is posted
//!   <home>/bundles/<hash>.tar     fleet bundles, verified before they are kept
//! ```
//!
//! What a crashed runner left under `sandboxes` is swept by the engine built on
//! it, which alone can kill a sandbox's cgroup and unmount its disk before the
//! directory goes; a plain removal here would empty a disk still mounted and
//! leave the cgroup and the loop device behind.

use std::fs;
use std::path::PathBuf;

use crate::error::Result;

const SANDBOXES: &str = "sandboxes";
const SPOOL: &str = "spool";
const BUNDLES: &str = "bundles";

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

    /// Where lease sandboxes live: the base every engine is built with, so its
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
}

#[cfg(test)]
#[path = "storage_home/tests.rs"]
mod tests;
