//! The host-local storage root, and what lives under it.
//!
//! ```text
//!   <home>/sandboxes/<run_id>/  each run's sandbox; the engine's base
//!   <home>/git/<workspace>/<owner>/<name>.git
//!                                 a bare mirror per bound repository
//!   <home>/staging/               each step's files on their way into or out
//!                                 of a sandbox (`crate::staging`)
//!   <home>/toolbox/               the toolbox an engine verifies and mounts:
//!                                 its release, images and mounts beneath it
//!                                 (`afr_image::ToolboxHome`)
//! ```
//!
//! What a crashed runner left under `sandboxes` is swept by the engine built on
//! it, which alone can kill a sandbox's cgroup and unmount its disk before the
//! directory goes; a plain removal here would empty a disk still mounted and
//! leave the cgroup and the loop device behind. What it left under `staging`
//! is plain files on the host, and opening the home removes it.

use std::fs;
use std::path::PathBuf;

use crate::error::Result;
use crate::staging::Staging;

const SANDBOXES: &str = "sandboxes";
const MIRRORS: &str = "git";
const TOOLBOX: &str = "toolbox";
const STAGING: &str = "staging";

/// The storage root, with its directories made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageHome {
    root: PathBuf,
}

impl StorageHome {
    /// Opens `root`, making it and its directories when they are missing,
    /// and empties its staging of what an earlier runner left.
    ///
    /// # Errors
    /// A directory that cannot be made, or staging that cannot be emptied.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let home = Self { root: root.into() };
        for directory in [home.sandboxes(), home.mirrors(), home.toolbox()] {
            fs::create_dir_all(directory)?;
        }
        Staging::sweep(&home.root.join(STAGING))?;
        Ok(home)
    }

    /// Where run sandboxes live: the base every engine is built with, so its
    /// boot sweep finds what a crashed runner left there.
    #[must_use]
    pub fn sandboxes(&self) -> PathBuf {
        self.root.join(SANDBOXES)
    }

    /// Where bound repositories are mirrored, outside every sandbox.
    pub(crate) fn mirrors(&self) -> PathBuf {
        self.root.join(MIRRORS)
    }

    /// Where each step's files wait on their way into or out of a sandbox.
    #[must_use]
    pub fn staging(&self) -> Staging {
        Staging::new(self.root.join(STAGING))
    }

    /// Where the toolbox is kept: the release a deploy stages, the images
    /// admitted from it, and their mounts, which the engine that verifies it
    /// lays out beneath.
    #[must_use]
    pub fn toolbox(&self) -> PathBuf {
        self.root.join(TOOLBOX)
    }
}

#[cfg(test)]
mod tests;
