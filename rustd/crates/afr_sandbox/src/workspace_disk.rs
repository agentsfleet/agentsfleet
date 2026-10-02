//! One lease's workspace: a sparse ext4 image, loop-mounted, sized to its limit.
//!
//! A block image rather than a quota on the host's file system, so it works on
//! any host and a full disk answers `ENOSPC` inside the sandbox; a microVM
//! engine attaches the same image as its data disk.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::host::{self, HostTools, MKE2FS, MOUNT};

/// The image's file name inside the lease's directory.
const IMAGE_NAME: &str = "workspace.img";
/// Where the image is mounted inside the lease's directory.
const MOUNT_DIR: &str = "workspace";
/// The file system every workspace is formatted with.
const EXT4: &str = "ext4";
/// The flag both `mke2fs` and `mount` take a file-system type after.
const TYPE_FLAG: &str = "-t";
/// Mount options: a loop device, and nothing on the disk may raise privilege.
const WORKSPACE_OPTIONS: &str = "loop,nosuid,nodev";
/// The event a leftover from a failed build is logged under.
const EVENT_DISK_LEFT: &str = "sandbox_workspace_left";

/// A mounted workspace disk. Releasing it is the only cleanup, and consumes it.
#[derive(Debug)]
pub struct WorkspaceDisk {
    image: PathBuf,
    mount_point: PathBuf,
}

impl WorkspaceDisk {
    /// Makes a sparse `bytes`-byte image in `dir`, formats it owned by `owner`,
    /// and mounts it.
    ///
    /// # Errors
    /// Any step fails; whatever the earlier steps made is removed first.
    pub async fn create(
        tools: &HostTools,
        dir: &Path,
        bytes: u64,
        owner: (u32, u32),
    ) -> Result<Self> {
        let disk = Self {
            image: dir.join(IMAGE_NAME),
            mount_point: dir.join(MOUNT_DIR),
        };
        match disk.build(tools, bytes, owner).await {
            Ok(()) => Ok(disk),
            Err(error) => {
                disk.discard();
                Err(error)
            }
        }
    }

    async fn build(&self, tools: &HostTools, bytes: u64, owner: (u32, u32)) -> Result<()> {
        fs::File::create(&self.image)?.set_len(bytes)?;
        host::run(MKE2FS, &tools.mke2fs, format_arguments(&self.image, owner)).await?;
        fs::create_dir(&self.mount_point)?;
        host::run(
            MOUNT,
            &tools.mount,
            mount_arguments(EXT4, WORKSPACE_OPTIONS, &self.image, &self.mount_point),
        )
        .await
    }

    /// Removes what a failed build left; nothing is mounted at this point.
    fn discard(&self) {
        for leftover in [
            fs::remove_dir(&self.mount_point),
            fs::remove_file(&self.image),
        ] {
            if let Err(error) = leftover.or_else(absent) {
                let reason = error.to_string();
                let event = EVENT_DISK_LEFT;
                tracing::warn!(reason, event, "a failed workspace disk left a file behind");
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

    /// Unmounts the disk, which frees its loop device, then deletes the image.
    ///
    /// # Errors
    /// The kernel refuses the unmount, or the files cannot be removed.
    #[cfg(target_os = "linux")]
    pub fn release(self) -> Result<()> {
        rustix::mount::unmount(&self.mount_point, rustix::mount::UnmountFlags::empty())?;
        fs::remove_dir(&self.mount_point)?;
        fs::remove_file(&self.image)?;
        Ok(())
    }
}

/// A file that was never made needs no removing.
fn absent(error: std::io::Error) -> std::io::Result<()> {
    if error.kind() == ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}

/// `mke2fs` for a quiet, journal-free ext4 whose root `owner` may write.
///
/// No journal: the disk outlives no crash, so a journal is writes for nothing.
pub(crate) fn format_arguments(image: &Path, owner: (u32, u32)) -> Vec<OsString> {
    let (user, group) = owner;
    [
        "-q",
        "-F",
        TYPE_FLAG,
        EXT4,
        "-m",
        "0",
        "-O",
        "^has_journal",
        "-E",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([
        format!("root_owner={user}:{group}").into(),
        image.as_os_str().to_owned(),
    ])
    .collect()
}

/// `mount -t fstype -o options source target`.
pub(crate) fn mount_arguments(
    fstype: &str,
    options: &str,
    source: &Path,
    target: &Path,
) -> Vec<OsString> {
    [TYPE_FLAG, fstype, "-o", options]
        .into_iter()
        .map(OsString::from)
        .chain([source.as_os_str().to_owned(), target.as_os_str().to_owned()])
        .collect()
}

#[cfg(test)]
mod tests;
