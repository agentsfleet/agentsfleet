//! The toolbox: a read-only EROFS image every lease sees as its root.
//!
//! Named by its SHA-256, verified before it is ever mounted, mounted once per
//! host and bound read-only into every sandbox. A lease path that never pulls an
//! image or unpacks a layer is where the start time comes from.

use std::fs;
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::error::{ErrorKind, Result};
use crate::host::{self, HostTools, MOUNT};
use crate::workspace_disk::mount_arguments;

/// Every image's file name starts with this.
pub const TOOLBOX_PREFIX: &str = "toolbox-";
/// Every image's file name ends with this.
pub const TOOLBOX_SUFFIX: &str = ".erofs";
/// The file system the image holds.
const EROFS: &str = "erofs";
/// Mount options: a loop device, read-only, nothing on it raises privilege.
const TOOLBOX_OPTIONS: &str = "loop,ro,nosuid,nodev";
/// Where the kernel lists this process's mounts.
const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";
/// The 0-based field of a `mountinfo` line that holds the mount point.
const MOUNTINFO_MOUNT_POINT: usize = 4;
/// Bytes hashed per read.
const HASH_CHUNK_BYTES: usize = 1 << 20;

/// An image whose bytes hash to the digest in its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolboxImage {
    path: PathBuf,
    digest: String,
}

impl ToolboxImage {
    /// Hashes the image and accepts it only when its bytes match its name.
    ///
    /// # Errors
    /// The file cannot be read, or its name states no digest or the wrong one.
    pub fn verify(path: &Path) -> Result<Self> {
        let actual = sha256_of(path)?;
        let named = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(TOOLBOX_PREFIX))
            .and_then(|name| name.strip_suffix(TOOLBOX_SUFFIX));
        if named != Some(actual.as_str()) {
            return Err(ErrorKind::ToolboxUnverified {
                path: path.to_owned(),
                actual,
            }
            .into());
        }
        Ok(Self {
            path: path.to_owned(),
            digest: actual,
        })
    }

    /// The image file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The SHA-256 its bytes hash to, in lowercase hexadecimal.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// A verified toolbox, mounted read-only on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolbox {
    root: PathBuf,
}

impl Toolbox {
    /// Mounts `image` at `<state>/<digest>`, or adopts the mount a previous
    /// run of this host left there: the directory's name is the image's hash,
    /// so whatever is mounted there is this image.
    ///
    /// # Errors
    /// The directory cannot be made or the mount is refused.
    pub async fn mount(image: &ToolboxImage, tools: &HostTools, state: &Path) -> Result<Self> {
        let root = state.join(image.digest());
        let mounts = fs::read_to_string(MOUNTINFO_PATH).unwrap_or_default();
        if !is_mount_point(&mounts, &root) {
            fs::create_dir_all(&root)?;
            host::run(
                MOUNT,
                &tools.mount,
                mount_arguments(EROFS, TOOLBOX_OPTIONS, image.path(), &root),
            )
            .await?;
        }
        Ok(Self { root })
    }

    /// Adopts a root file system already in place, for an engine whose root
    /// is not an image of its own.
    #[must_use]
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// Where it is mounted.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Detaches the host's mount of the image and removes its mount point.
    /// Sandboxes already started keep their own bind of it.
    ///
    /// # Errors
    /// The kernel refuses the unmount, or the mount point cannot be removed.
    #[cfg(target_os = "linux")]
    pub fn unmount(self) -> Result<()> {
        rustix::mount::unmount(&self.root, rustix::mount::UnmountFlags::empty())?;
        fs::remove_dir(&self.root)?;
        Ok(())
    }
}

/// The image's SHA-256, read in chunks so a large image never sits in memory.
fn sha256_of(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(HASH_CHUNK_BYTES, fs::File::open(path)?);
    let mut hasher = Sha256::new();
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(chunk);
        let read = chunk.len();
        reader.consume(read);
    }
}

/// Whether `/proc/self/mountinfo` text lists `path` as a mount point.
pub(crate) fn is_mount_point(mountinfo: &str, path: &Path) -> bool {
    mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(MOUNTINFO_MOUNT_POINT))
        .any(|point| Path::new(point) == path)
}

#[cfg(test)]
mod tests;
