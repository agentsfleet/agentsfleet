//! Adoption: a toolbox mount a previous run left at a digest's directory is
//! used only when it is the admitted image, mounted the way admission mounts
//! it. A directory named after a digest proves nothing: an operator, a crashed
//! runner or a tamperer could have put any file system there.

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

use procfs_core::FromBufRead as _;
use procfs_core::process::MountInfos;

use super::loop_device::{self, LO_FLAGS_READ_ONLY};
use crate::error::Result;
use crate::mounts;
use crate::probe::MECHANISM_TOOLBOX_FILESYSTEM as EROFS;

/// Where the kernel lists this process's mounts.
const MOUNTINFO: &str = "/proc/self/mountinfo";
/// The options every toolbox mount carries.
const REQUIRED_OPTIONS: [&str; 3] = ["ro", "nosuid", "nodev"];
/// The root a mount of a whole file system shows: anything else is a bind of
/// a directory inside one.
const WHOLE_FILE_SYSTEM: &str = "/";
/// The block-device major number every loop device has.
const LOOP_MAJOR: u32 = 7;
/// Where the kernel publishes every block device by number, and the file
/// naming a device's node.
const SYS_DEV_BLOCK: &str = "/sys/dev/block";
const UEVENT: &str = "uevent";
/// The line of a device's `uevent` that names its node under `/dev`.
const DEVNAME: &str = "DEVNAME=";
/// The event an adopted mount is logged under.
const EVENT_ADOPTED: &str = "sandbox_toolbox_adopted";
/// The event a foreign mount at a toolbox root is logged under as it goes.
const EVENT_FOREIGN_DETACHED: &str = "sandbox_toolbox_foreign_detached";

/// Whether the mount at `root` is the image whose file is `image` (its device
/// and inode); anything else mounted there is detached, so admission mounts
/// afresh. Sandboxes still using a detached mount keep it until they let go.
pub(crate) fn adopt(root: &Path, image: (u64, u64)) -> Result<bool> {
    if !mounts::is_mount_root(root) {
        return Ok(false);
    }
    let found = mismatch(root, image)?;
    let shown = root.display().to_string();
    match found {
        None => {
            let event = EVENT_ADOPTED;
            tracing::info!(root = shown, event);
            Ok(true)
        }
        Some(reason) => {
            let event = EVENT_FOREIGN_DETACHED;
            tracing::warn!(root = shown, reason, event);
            mounts::unmount(root, true)?;
            Ok(false)
        }
    }
}

/// Why the mount at `root` is not the admitted image, if it is not.
fn mismatch(root: &Path, image: (u64, u64)) -> Result<Option<String>> {
    let canonical = fs::canonicalize(root)?;
    let mounts = MountInfos::from_buf_read(BufReader::new(File::open(MOUNTINFO)?))
        .map_err(io::Error::other)?;
    // The last mount at a point is the one a path there resolves to.
    let Some(mount) = mounts
        .0
        .iter()
        .rev()
        .find(|mount| mount.mount_point == canonical)
    else {
        return Ok(Some("the kernel lists no mount there".to_owned()));
    };
    if mount.fs_type != EROFS {
        return Ok(Some(format!("a {} file system", mount.fs_type)));
    }
    if mount.root != WHOLE_FILE_SYSTEM {
        return Ok(Some(format!(
            "a bind of {} inside its file system",
            mount.root
        )));
    }
    // A sandbox binds the toolbox root with everything under it.
    if mounts
        .0
        .iter()
        .any(|other| other.mount_point != canonical && other.mount_point.starts_with(&canonical))
    {
        return Ok(Some("a mount stacked beneath it".to_owned()));
    }
    if let Some(missing) = REQUIRED_OPTIONS
        .iter()
        .find(|option| !mount.mount_options.contains_key(**option))
    {
        return Ok(Some(format!("mounted without {missing}")));
    }
    let device = rustix::fs::stat(&canonical)?.st_dev;
    let (major, minor) = (rustix::fs::major(device), rustix::fs::minor(device));
    if major != LOOP_MAJOR {
        return Ok(Some(format!(
            "on device {major}:{minor}, not a loop device"
        )));
    }
    let backing = loop_device::backing(&loop_node(major, minor)?)?;
    Ok(backing_mismatch(&backing, image))
}

/// Why a loop device attached as `backing` is not the admitted `image`
/// shown whole and read-only, if it is not.
fn backing_mismatch(backing: &loop_device::Backing, image: (u64, u64)) -> Option<String> {
    let same_device = |encoded: u64, stat: u64| {
        (rustix::fs::major(encoded), rustix::fs::minor(encoded))
            == (rustix::fs::major(stat), rustix::fs::minor(stat))
    };
    if !same_device(backing.file.0, image.0) || backing.file.1 != image.1 {
        return Some("a loop device of another file".to_owned());
    }
    if backing.flags & LO_FLAGS_READ_ONLY == 0 || backing.offset != 0 || backing.size_limit != 0 {
        return Some("a loop device that is writable or shows part of its file".to_owned());
    }
    None
}

/// The node of loop device `major:minor`, as the kernel names it.
fn loop_node(major: u32, minor: u32) -> Result<PathBuf> {
    let uevent = fs::read_to_string(
        Path::new(SYS_DEV_BLOCK)
            .join(format!("{major}:{minor}"))
            .join(UEVENT),
    )?;
    uevent
        .lines()
        .find_map(|line| line.strip_prefix(DEVNAME))
        .map(|name| Path::new("/dev").join(name))
        .ok_or_else(|| {
            io::Error::other(format!("loop device {major}:{minor} names no node")).into()
        })
}

#[cfg(test)]
#[path = "adopt/tests.rs"]
mod tests;
