//! One run's workspace: a sparse ext4 image, sized to its limit, which a
//! microVM engine attaches as its guest's data drive.
//!
//! A block image rather than a quota on the host's file system, so it works on
//! any host and a full disk answers `ENOSPC` inside the sandbox.

use std::fs;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use crate::error::Result;
use crate::host::HostTools;

/// Readable and writable by its owner alone: the raw image is every file the
/// run wrote.
const IMAGE_MODE: u32 = 0o600;

/// How an image's blocks are cached on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caching {
    /// Its loop device reads and writes the image directly: the blocks are
    /// cached once.
    Direct,
    /// The host's page cache holds the image's blocks as well: the backing
    /// file system refused direct I/O.
    Buffered,
}

impl Caching {
    /// The spelling a log field carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Buffered => "buffered",
        }
    }
}

/// Makes and formats a sparse `bytes`-byte ext4 image at `image`.
///
/// The image is readable by its owner alone and its root writable by `owner`:
/// a workspace disk no host mounts, which a microVM engine attaches as its
/// guest's data drive.
///
/// # Errors
/// The file exists already, cannot be made or sized, or `mke2fs` fails.
pub async fn format_image(
    tools: &HostTools,
    image: &Path,
    bytes: u64,
    owner: (u32, u32),
) -> Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(IMAGE_MODE)
        .open(image)?
        .set_len(bytes)?;
    tools.format_ext4(image, owner).await
}

#[cfg(test)]
mod tests;
