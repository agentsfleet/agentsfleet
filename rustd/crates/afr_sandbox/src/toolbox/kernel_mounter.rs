//! The mounter a Linux host admits toolboxes with: the kernel's loop devices
//! and EROFS mounts, which only the kernel lane can exercise. It lives in its
//! own file so a coverage gate can excuse exactly this code and nothing
//! beside it.

use std::path::{Path, PathBuf};

use super::{Manifest, Mounter, Toolbox};
use crate::error::Result;

/// The admission [`Toolbox::admit`] makes, mounting under `mounts`.
#[derive(Debug)]
pub struct KernelMounter {
    mounts: PathBuf,
}

impl KernelMounter {
    /// Mounts each admitted image at `<mounts>/<digest>`.
    #[must_use]
    pub fn new(mounts: PathBuf) -> Self {
        Self { mounts }
    }
}

impl Mounter for KernelMounter {
    fn mount(&self, manifest: &Manifest, image: &Path) -> Result<Toolbox> {
        Toolbox::admit_now(manifest, image, &self.mounts)
    }

    fn unmount(&self, toolbox: &Toolbox) -> Result<()> {
        toolbox.unmount()
    }
}
