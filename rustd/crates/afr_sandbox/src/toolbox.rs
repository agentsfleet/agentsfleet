//! The toolbox: a read-only EROFS image every lease sees as its root.
//!
//! Named by its SHA-256, verified before it is mounted, verified again once
//! mounted, and bound read-only into every sandbox. A lease path that never
//! pulls an image or unpacks a layer is where the start time comes from.
//!
//! # Why the mount is verified, not only the file
//!
//! `mount -o loop` opens the image by path, after the hash was taken. A file
//! swapped in between would be what every sandbox runs. So the mounted loop
//! device's own bytes are hashed too: what is verified is what is mounted.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use digest_io::IoWrapper;
use sha2::{Digest as _, Sha256};

use crate::error::{ErrorKind, Result};
use crate::host::{self, HostTools, MOUNT};
use crate::probe::MECHANISM_TOOLBOX_FILESYSTEM as EROFS;
use crate::workspace_disk::mount_arguments;

/// Every image's file name starts with this.
pub const TOOLBOX_PREFIX: &str = "toolbox-";
/// Every image's file name ends with this.
pub const TOOLBOX_SUFFIX: &str = ".erofs";
/// Mount options: a loop device, read-only, nothing on it raises privilege.
const TOOLBOX_OPTIONS: &str = "loop,ro,nosuid,nodev";
/// The block-device major number every loop device has.
#[cfg(target_os = "linux")]
const LOOP_MAJOR: u32 = 7;
/// Where the kernel publishes every block device by number.
#[cfg(target_os = "linux")]
const SYS_DEV_BLOCK: &str = "/sys/dev/block";
/// The file naming a device's node.
#[cfg(target_os = "linux")]
const UEVENT: &str = "uevent";
/// Why a root mounted from anything but a loop device is refused.
#[cfg(target_os = "linux")]
const NOT_A_LOOP_DEVICE: &str = "the toolbox root is not mounted from a loop device";
/// The event a toolbox mount's start is logged under.
const EVENT_MOUNT_STARTED: &str = "sandbox_toolbox_mount_started";
/// The event a verified toolbox mount is logged under.
const EVENT_MOUNT_COMPLETED: &str = "sandbox_toolbox_mount_completed";
/// The event a toolbox that could not be mounted or verified is logged under.
const EVENT_MOUNT_FAILED: &str = "sandbox_toolbox_mount_failed";

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
    digest: String,
}

impl Toolbox {
    /// Mounts `image` at `<state>/<digest>` and verifies what got mounted.
    ///
    /// A mount a previous run left there is kept only if its device still
    /// hashes to the image's digest; otherwise it is detached — sandboxes
    /// still using it keep it — and the image is mounted afresh.
    ///
    /// # Errors
    /// The directory cannot be made, the mount is refused, or the mounted
    /// device's bytes are not the image's.
    pub async fn mount(image: &ToolboxImage, tools: &HostTools, state: &Path) -> Result<Self> {
        let digest = image.digest();
        let event = EVENT_MOUNT_STARTED;
        tracing::info!(digest, event);
        let mounted = Self::attach(image, tools, state).await;
        match &mounted {
            Ok(_) => {
                let event = EVENT_MOUNT_COMPLETED;
                tracing::info!(digest, event);
            }
            Err(error) => {
                let error_code = error.code().as_str();
                let reason = error.to_string();
                let event = EVENT_MOUNT_FAILED;
                tracing::error!(digest, error_code, reason, event);
            }
        }
        mounted
    }

    async fn attach(image: &ToolboxImage, tools: &HostTools, state: &Path) -> Result<Self> {
        let toolbox = Self {
            root: state.join(image.digest()),
            digest: image.digest().to_owned(),
        };
        // A mount already there is adopted only if it is this image; one that
        // is a loop device of another image is detached as it is verified,
        // and anything else is mounted over.
        if crate::mounts::is_mount_root(&toolbox.root) && toolbox.verify_mounted().is_ok() {
            return Ok(toolbox);
        }
        fs::create_dir_all(&toolbox.root)?;
        host::run(
            MOUNT,
            &tools.mount,
            mount_arguments(EROFS, TOOLBOX_OPTIONS, image.path(), &toolbox.root),
        )
        .await?;
        toolbox.verify_mounted().map(|()| toolbox)
    }

    /// Hashes the block device mounted at the root and refuses unless it is
    /// exactly the image this toolbox is named for.
    #[cfg(target_os = "linux")]
    fn verify_mounted(&self) -> Result<()> {
        let device = rustix::fs::stat(&self.root)?.st_dev;
        let (major, minor) = (rustix::fs::major(device), rustix::fs::minor(device));
        // What is mounted is not what was verified; nothing may use it.
        loop_node(Path::new(SYS_DEV_BLOCK), major, minor)
            .and_then(|node| self.verify_device(&node))
            .inspect_err(|_unverified| self.detach())
    }

    /// Refuses unless the device node's bytes are exactly this toolbox's image.
    #[cfg(target_os = "linux")]
    fn verify_device(&self, node: &Path) -> Result<()> {
        let actual = sha256_of(node)?;
        if actual == self.digest {
            Ok(())
        } else {
            Err(ErrorKind::ToolboxUnverified {
                path: node.to_owned(),
                actual,
            }
            .into())
        }
    }

    /// Detaches whatever is mounted at the root, logging a refusal: the
    /// mount is wrong whether or not it goes.
    #[cfg(target_os = "linux")]
    fn detach(&self) {
        crate::mounts::unmount(&self.root, true).unwrap_or_else(|error| {
            let error_code = error.code().as_str();
            let reason = error.to_string();
            let event = EVENT_MOUNT_FAILED;
            tracing::error!(error_code, reason, event, "a wrong toolbox stayed mounted");
        });
    }

    /// Off Linux nothing is ever mounted, so nothing mounted can be verified.
    #[cfg(not(target_os = "linux"))]
    #[expect(
        clippy::unused_self,
        reason = "the Linux build reads the root; this one has none to read"
    )]
    fn verify_mounted(&self) -> Result<()> {
        Err(crate::error::refused(EROFS))
    }

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

    /// Detaches the host's mount of the image and removes its mount point.
    /// Sandboxes already started keep their own bind of it.
    ///
    /// # Errors
    /// The kernel refuses the unmount, or the mount point cannot be removed.
    #[cfg(target_os = "linux")]
    pub fn unmount(self) -> Result<()> {
        crate::mounts::unmount(&self.root, false)
            .and_then(|()| fs::remove_dir(&self.root).map_err(Into::into))
    }
}

/// The node of loop device `major:minor`, as the kernel names it. Only a loop
/// device is ever hashed: anything else mounted at a toolbox root is not an
/// image, and hashing it could mean reading a whole disk.
#[cfg(target_os = "linux")]
fn loop_node(sys_dev_block: &Path, major: u32, minor: u32) -> Result<PathBuf> {
    /// The line of a device's `uevent` that names its node under `/dev`.
    const DEVNAME: &str = "DEVNAME=";
    if major != LOOP_MAJOR {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, NOT_A_LOOP_DEVICE).into());
    }
    let uevent = fs::read_to_string(sys_dev_block.join(format!("{major}:{minor}")).join(UEVENT))?;
    uevent
        .lines()
        .find_map(|line| line.strip_prefix(DEVNAME))
        .map(|name| Path::new("/dev").join(name))
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound).into())
}

/// The SHA-256 of everything readable at `path`, streamed through the hasher.
fn sha256_of(path: &Path) -> Result<String> {
    let mut hasher = IoWrapper(Sha256::new());
    io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(hex::encode(hasher.0.finalize()))
}

#[cfg(test)]
mod tests;
