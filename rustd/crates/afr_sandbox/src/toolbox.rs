//! The toolbox: a read-only EROFS image every lease sees as its root.
//!
//! A host admits an image in the order `docs/architecture/runner_execution.md`
//! §Toolbox gives: the release manifest's signature and facts
//! ([`Release::verify`]); the image staged in a private directory, verified,
//! synced and published with a rename ([`Toolboxes::admit`]); then the
//! published file opened once without following a link, checked to be a
//! regular file of the manifest's length, hashed through that descriptor,
//! attached read-only to a loop device from the same descriptor, and mounted
//! `ro,nosuid,nodev` ([`Toolbox::admit`]). Nothing reopens the image by path
//! after its hash is taken, so a file substituted at that path never reaches
//! the kernel's file-system parser.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Result;

#[cfg(target_os = "linux")]
mod adopt;
mod holds;
mod incoming;
#[cfg(target_os = "linux")]
mod kernel_mounter;
#[cfg(target_os = "linux")]
pub(crate) mod loop_device;
mod manifest;
mod stage;
#[cfg(test)]
mod testing;

pub use self::holds::{Mounter, TOOLBOX_KEEP_RELEASES, Toolboxes};
pub use self::incoming::{MANIFEST_SUFFIX, SIGNATURE_SUFFIX};
#[cfg(target_os = "linux")]
pub use self::kernel_mounter::KernelMounter;
pub use self::manifest::{Manifest, Release, TOOLBOX_RELEASE_PUBLIC_KEY};

/// The three directories of a toolbox home, by name.
const INCOMING: &str = "incoming";
const IMAGES: &str = "images";
const MOUNTS: &str = "mounts";

/// Every image's file name starts with this.
pub const TOOLBOX_PREFIX: &str = "toolbox-";
/// Every image's file name ends with this.
pub const TOOLBOX_SUFFIX: &str = ".erofs";

/// The name an image of digest `digest` is published under.
pub(crate) fn image_name(digest: &str) -> String {
    format!("{TOOLBOX_PREFIX}{digest}{TOOLBOX_SUFFIX}")
}

/// Whether `name` is a published image's name, of any digest.
pub(crate) fn is_image_name(name: &str) -> bool {
    name.starts_with(TOOLBOX_PREFIX) && name.ends_with(TOOLBOX_SUFFIX)
}

/// Where one host keeps its toolbox.
///
/// The release a deploy stages, the images admitted from it, and where each
/// is mounted are three siblings, so nothing a deploy stages lands where an
/// admitted image is published or mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolboxHome {
    root: PathBuf,
}

impl ToolboxHome {
    /// The home at `root`, as it stands.
    #[must_use]
    pub const fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The home at `root`, its directories made when they are missing, as a
    /// host's are before anything is staged into them.
    ///
    /// # Errors
    /// A directory that cannot be made.
    pub fn open(root: PathBuf) -> Result<Self, io::Error> {
        let home = Self::at(root);
        for directory in [home.incoming(), home.images(), home.mounts()] {
            fs::create_dir_all(directory)?;
        }
        Ok(home)
    }

    /// Where a deploy stages the release this host admits at boot.
    #[must_use]
    pub fn incoming(&self) -> PathBuf {
        self.root.join(INCOMING)
    }

    /// Where admitted images are published, each under its digest.
    #[must_use]
    pub fn images(&self) -> PathBuf {
        self.root.join(IMAGES)
    }

    /// Where each admitted image is mounted.
    #[must_use]
    pub fn mounts(&self) -> PathBuf {
        self.root.join(MOUNTS)
    }
}

/// An admitted toolbox, mounted read-only on the host. A sandbox holds the one
/// it runs on by keeping a clone of its `Arc` until it is destroyed.
#[derive(Debug, PartialEq, Eq)]
pub struct Toolbox {
    root: PathBuf,
    digest: String,
}

impl Toolbox {
    /// Adopts a root file system already in place under the digest it is
    /// known by, for an engine whose root is not a mounted image of its own.
    #[must_use]
    pub fn at(root: PathBuf, digest: String) -> Self {
        Self { root, digest }
    }

    /// Where it is mounted.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The SHA-256 of the image it was mounted from.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[cfg(target_os = "linux")]
mod admit;

#[cfg(test)]
mod tests;
