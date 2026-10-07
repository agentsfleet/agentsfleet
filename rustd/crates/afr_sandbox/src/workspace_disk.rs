//! One lease's workspace: a sparse ext4 image, loop-mounted, sized to its limit.
//!
//! A block image rather than a quota on the host's file system, so it works on
//! any host and a full disk answers `ENOSPC` inside the sandbox; a microVM
//! engine attaches the same image as its data disk. The disk holds two
//! directories, `workspace/` and `tmp/`, bound at `/workspace` and `/tmp`
//! inside the sandbox: both draw on the one limit, so a full `/tmp` answers
//! `ENOSPC` like a full workspace, where a tmpfs would hold its pages against
//! the lease's memory until its last process is gone; and `mke2fs`'s
//! `lost+found` stays out of `/workspace`.

use std::fs::{self, DirBuilder, Permissions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use afd_core::error_code;
use rustix::fs::{Gid, Uid};

use crate::error::Result;
use crate::host::{EXT4, HostTools};
#[cfg(target_os = "linux")]
use crate::toolbox::loop_device::{self, LOOP_MAJOR};

/// The image's file name inside the lease's directory.
const IMAGE_NAME: &str = "workspace.img";
/// Where the image is mounted inside the lease's directory.
const MOUNT_DIR: &str = "workspace";
/// Mount options: a loop device, and nothing on the disk may raise privilege.
const WORKSPACE_OPTIONS: &str = "loop,nosuid,nodev";
/// The image's permissions: root reads and writes it, nobody else.
const IMAGE_MODE: u32 = 0o600;
/// The directory on the disk bound at `/workspace`.
pub(crate) const WORKSPACE_DIR: &str = "workspace";
/// The directory on the disk bound at `/tmp`.
pub(crate) const TMP_DIR: &str = "tmp";
/// The workspace's mode: the sandbox user's, readable by the rest.
const WORKSPACE_MODE: u32 = 0o755;
/// `/tmp`'s mode: every user writes, and only an owner removes.
const TMP_MODE: u32 = 0o1777;
/// The event a leftover from a failed build is logged under.
const EVENT_DISK_LEFT: &str = "sandbox_workspace_left";

/// How a workspace disk's blocks are cached on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caching {
    /// Its loop device reads and writes the image directly: the blocks are
    /// cached once, in the sandbox's own memory.
    Direct,
    /// The host's page cache holds the image's blocks as well: the backing
    /// file system refused direct I/O, or the mount made no loop device.
    Buffered,
}

/// A mounted workspace disk. Releasing it is the only cleanup, and consumes it.
#[derive(Debug)]
pub struct WorkspaceDisk {
    image: PathBuf,
    mount_point: PathBuf,
    workspace: PathBuf,
    tmp: PathBuf,
}

impl WorkspaceDisk {
    /// Makes a sparse `bytes`-byte image in `dir`, formats it owned by `owner`,
    /// and mounts it; says how the host caches it.
    ///
    /// # Errors
    /// Any step fails; whatever the earlier steps made is removed first.
    pub async fn create(
        tools: &HostTools,
        dir: &Path,
        bytes: u64,
        owner: (u32, u32),
    ) -> Result<(Self, Caching)> {
        let disk = Self::in_dir(dir);
        match disk.build(tools, bytes, owner).await {
            Ok(caching) => Ok((disk, caching)),
            Err(error) => {
                disk.discard();
                Err(error)
            }
        }
    }

    async fn build(&self, tools: &HostTools, bytes: u64, owner: (u32, u32)) -> Result<Caching> {
        // Readable by root alone: the raw image is every file the lease wrote.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(IMAGE_MODE)
            .open(&self.image)?
            .set_len(bytes)?;
        tools.format_ext4(&self.image, owner).await?;
        fs::create_dir(&self.mount_point)?;
        tools
            .mount(EXT4, WORKSPACE_OPTIONS, &self.image, &self.mount_point)
            .await?;
        #[cfg(target_os = "linux")]
        let caching = self.direct_io()?;
        #[cfg(not(target_os = "linux"))]
        let caching = Caching::Buffered;
        lay_out(&self.mount_point, owner)?;
        Ok(caching)
    }

    /// The disk's paths under `dir`, made or not.
    fn in_dir(dir: &Path) -> Self {
        let mount_point = dir.join(MOUNT_DIR);
        Self {
            image: dir.join(IMAGE_NAME),
            workspace: mount_point.join(WORKSPACE_DIR),
            tmp: mount_point.join(TMP_DIR),
            mount_point,
        }
    }

    /// The directory on the disk the sandbox sees as `/workspace`.
    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The directory on the disk the sandbox sees as `/tmp`.
    #[must_use]
    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    /// Removes what a failed build left; nothing is mounted at this point.
    fn discard(&self) {
        for (path, leftover) in [
            (&self.mount_point, fs::remove_dir(&self.mount_point)),
            (&self.image, fs::remove_file(&self.image)),
        ] {
            if let Err(error) = leftover.or_else(absent) {
                let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let path = path.display();
                let event = EVENT_DISK_LEFT;
                tracing::warn!(
                    error_code,
                    %path,
                    reason = %error,
                    event,
                    "a failed workspace disk left a file behind"
                );
            }
        }
    }

    /// Where the disk is mounted on the host.
    #[must_use]
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// The loop device's `major:minor`, which `io.max` limits.
    ///
    /// # Errors
    /// The mount point cannot be read.
    #[cfg(target_os = "linux")]
    pub fn device(&self) -> Result<(u32, u32)> {
        let device = rustix::fs::stat(&self.mount_point)?.st_dev;
        Ok((rustix::fs::major(device), rustix::fs::minor(device)))
    }

    /// Switches the loop device the mount made to direct I/O, so the image is
    /// cached once, in the sandbox's own memory, rather than again on the
    /// host. A mount helper that made no loop device has none to switch.
    #[cfg(target_os = "linux")]
    fn direct_io(&self) -> Result<Caching> {
        match self.device()? {
            (major, minor) if major == LOOP_MAJOR => {
                loop_device::direct_io(&loop_device::node(major, minor)?)
            }
            _ => Ok(Caching::Buffered),
        }
    }

    /// Unmounts the disk, which frees its loop device, then deletes the image.
    ///
    /// The image is deleted only once nothing is mounted from it: an unmount
    /// the kernel refuses leaves both the mount point and the image in place,
    /// so a loop device is never left attached to a file nobody can name.
    ///
    /// # Errors
    /// The kernel refuses the unmount, or the files cannot be removed.
    #[cfg(target_os = "linux")]
    pub fn release(self) -> Result<()> {
        crate::mounts::unmount(&self.mount_point, false).and_then(|()| self.remove_files())
    }

    /// Adopts whatever disk a previous run left in `dir`, mounted or not.
    #[cfg(target_os = "linux")]
    pub(crate) fn leftover(dir: &Path) -> Self {
        Self::in_dir(dir)
    }

    /// Releases a leftover: unmounted first only if something is mounted, and
    /// a file already gone is not a failure.
    #[cfg(target_os = "linux")]
    pub(crate) fn release_leftover(self) -> Result<()> {
        crate::mounts::is_mount_root(&self.mount_point)
            .then(|| crate::mounts::unmount(&self.mount_point, false))
            .transpose()?;
        self.remove_files()
    }

    #[cfg(target_os = "linux")]
    fn remove_files(&self) -> Result<()> {
        fs::remove_dir(&self.mount_point).or_else(absent)?;
        fs::remove_file(&self.image).or_else(absent)?;
        Ok(())
    }
}

/// Makes `workspace/` and `tmp/` on the disk mounted at `root`, each with its
/// mode whatever the umask, owned by `owner` like the disk's root.
///
/// # Errors
/// A directory cannot be made, given its mode, or given its owner.
pub(crate) fn lay_out(root: &Path, owner: (u32, u32)) -> Result<()> {
    let (uid, gid) = (Uid::from_raw(owner.0), Gid::from_raw(owner.1));
    for (name, mode) in [(WORKSPACE_DIR, WORKSPACE_MODE), (TMP_DIR, TMP_MODE)] {
        let dir = root.join(name);
        DirBuilder::new().mode(mode).create(&dir)?;
        fs::set_permissions(&dir, Permissions::from_mode(mode))?;
        rustix::fs::chown(&dir, Some(uid), Some(gid)).map_err(std::io::Error::from)?;
    }
    Ok(())
}

/// A file that was never made needs no removing.
fn absent(error: std::io::Error) -> std::io::Result<()> {
    if error.kind() == ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests;
