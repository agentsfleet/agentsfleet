//! One lease's workspace: a sparse ext4 image, loop-mounted in-process, sized
//! to its limit.
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

use afd_core::error_code::{Coded as _, Logged};
use rustix::fs::{Gid, Uid};

use crate::error::Result;
#[cfg(target_os = "linux")]
use crate::host::EXT4;
use crate::host::HostTools;
#[cfg(target_os = "linux")]
use crate::toolbox::loop_device::{self, Access};

/// The image's file name inside the lease's directory.
const IMAGE_NAME: &str = "workspace.img";
/// Where the image is mounted inside the lease's directory.
const MOUNT_DIR: &str = "workspace";
/// Why a workspace disk cannot be built off Linux.
#[cfg(not(target_os = "linux"))]
const NO_LOOP_DEVICES: &str = "no loop devices: only Linux mounts a workspace disk";
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
    /// file system refused direct I/O.
    Buffered,
}

/// Attaches a workspace image to a loop device and mounts it, blocking its
/// thread; says how the host caches it. [`loop_mount`] in production; a test
/// swaps in one that mounts nothing.
pub type Mount = fn(image: &Path, mount_point: &Path) -> Result<Caching>;

/// A disk the blocking pool mounted. The start that asked takes it with
/// [`Mounted::heard`]. A start dropped mid-mount never does: tokio drops this
/// output in its place, so the disk is undone where it lands, since nothing
/// else knows it is mounted.
struct Mounted {
    disk: Option<WorkspaceDisk>,
    caching: Caching,
}

/// A mounted workspace disk. Releasing it is the only cleanup, and consumes it.
#[derive(Debug)]
pub struct WorkspaceDisk {
    image: PathBuf,
    mount_point: PathBuf,
    workspace: PathBuf,
    tmp: PathBuf,
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

impl WorkspaceDisk {
    /// Makes a sparse `bytes`-byte image in `dir`, formats it owned by `owner`,
    /// and mounts it; says how the host caches it.
    ///
    /// # Errors
    /// Any step fails, and that step's failure is what returns. Whatever the
    /// build made is undone as a leftover is released: unmounted first where
    /// anything is mounted, then its files removed.
    pub async fn create(
        tools: &HostTools,
        dir: &Path,
        bytes: u64,
        owner: (u32, u32),
    ) -> Result<(Self, Caching)> {
        let disk = Self::in_dir(dir);
        let built = disk.make_and_mount(tools, bytes, owner).await;
        match built.and_then(|caching| lay_out(&disk.mount_point, owner).map(|()| caching)) {
            Ok(caching) => Ok((disk, caching)),
            Err(error) => Err(disk.undo(error)),
        }
    }

    /// Makes the sparse image, formats it and mounts it; says how the host
    /// caches it.
    async fn make_and_mount(
        &self,
        tools: &HostTools,
        bytes: u64,
        owner: (u32, u32),
    ) -> Result<Caching> {
        // Readable by root alone: the raw image is every file the lease wrote.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(IMAGE_MODE)
            .open(&self.image)?
            .set_len(bytes)?;
        tools.format_ext4(&self.image, owner).await?;
        fs::create_dir(&self.mount_point)?;
        let (mount, disk) = (tools.mount, self.paths());
        // Off the async runtime: the lease's build shares its task with the
        // model's stream, and an ioctl must not stall it.
        let mounted = tokio::task::spawn_blocking(move || {
            mount(&disk.image, &disk.mount_point).map(|caching| Mounted {
                disk: Some(disk),
                caching,
            })
        })
        .await??;
        Ok(mounted.heard())
    }

    /// Undoes a disk whose build failed, the way a leftover is released:
    /// unmounted where anything is mounted, which frees its loop device, then
    /// its files removed. An image is never unlinked while it is mounted;
    /// what cannot be undone is logged and kept for the boot sweep. Hands
    /// `failure`, the step's own, back unchanged.
    fn undo(self, failure: crate::Error) -> crate::Error {
        self.discard();
        failure
    }

    /// Unmounts the disk where anything is mounted, then removes its files;
    /// logs what it could not undo, which the boot sweep finds.
    fn discard(self) {
        let mount_point = self.mount_point.clone();
        #[cfg(target_os = "linux")]
        let undone = self.release_leftover();
        // No other kernel mounts a workspace disk: a mount that succeeded
        // here is a test's fake, so removing the files is the whole undo.
        #[cfg(not(target_os = "linux"))]
        let undone = self.remove_files();
        if let Err(leftover) = undone {
            let Logged { error_code, reason } = leftover.logged();
            let path = mount_point.display();
            let event = EVENT_DISK_LEFT;
            tracing::warn!(
                error_code,
                %path,
                reason,
                event,
                "a failed workspace disk could not be unmounted and removed"
            );
        }
    }

    /// The same disk's paths, for the blocking pool to own.
    fn paths(&self) -> Self {
        Self {
            image: self.image.clone(),
            mount_point: self.mount_point.clone(),
            workspace: self.workspace.clone(),
            tmp: self.tmp.clone(),
        }
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

    /// The loop device's `major:minor`, which `io.max` limits.
    ///
    /// # Errors
    /// The mount point cannot be read.
    #[cfg(target_os = "linux")]
    pub fn device(&self) -> Result<(u32, u32)> {
        let device = rustix::fs::stat(&self.mount_point)?.st_dev;
        Ok((rustix::fs::major(device), rustix::fs::minor(device)))
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

    /// Removes the mount point, with whatever was written to it while nothing
    /// was mounted there — nothing once a disk is unmounted, the layout where
    /// a mount made no disk — then the image.
    fn remove_files(&self) -> Result<()> {
        fs::remove_dir_all(&self.mount_point).or_else(absent)?;
        fs::remove_file(&self.image).or_else(absent)?;
        Ok(())
    }
}

impl Mounted {
    /// The start heard the mount, so the disk is its own to release.
    fn heard(mut self) -> Caching {
        self.disk = None;
        self.caching
    }
}

impl Drop for Mounted {
    fn drop(&mut self) {
        if let Some(disk) = self.disk.take() {
            disk.discard();
        }
    }
}

/// Attaches `image` read-write with direct I/O asked for, and mounts it at
/// `mount_point` `nosuid,nodev` while that handle still holds the device.
/// Closing the handle then leaves the mount the device's only holder, so the
/// unmount frees it.
///
/// # Errors
/// The image cannot be opened, no loop device attaches it, or the kernel
/// refuses the mount.
#[cfg(target_os = "linux")]
pub(crate) fn loop_mount(image: &Path, mount_point: &Path) -> Result<Caching> {
    let image = fs::OpenOptions::new().read(true).write(true).open(image)?;
    let attached = loop_device::attach(&image, Access::ReadWrite)?;
    let flags = rustix::mount::MountFlags::NOSUID | rustix::mount::MountFlags::NODEV;
    rustix::mount::mount(&attached.node, mount_point, EXT4, flags, None)?;
    Ok(attached.caching)
}

/// No other kernel has loop devices, so the build stops here.
///
/// # Errors
/// Always: only Linux mounts a workspace disk.
#[cfg(not(target_os = "linux"))]
pub(crate) fn loop_mount(_image: &Path, _mount_point: &Path) -> Result<Caching> {
    Err(crate::error::unconfined(NO_LOOP_DEVICES))
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
