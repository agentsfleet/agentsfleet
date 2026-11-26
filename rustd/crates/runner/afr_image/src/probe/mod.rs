//! What this host's kernel offers an image: whether it mounts the toolbox's
//! file system, and whether the state file system takes direct I/O.
//!
//! Each fact is read from a path the caller can replace, so a test states a
//! host rather than needing one.

use std::fs;
use std::path::Path;

use afr_engine::MECHANISM_TOOLBOX_FILESYSTEM;

/// Where the kernel lists the file systems it can mount.
pub const FILESYSTEMS_PATH: &str = "/proc/filesystems";

/// Whether the kernel's file-system list at `filesystems` names the
/// toolbox's (EROFS). A list that cannot be read names none.
#[must_use]
pub fn toolbox_filesystem(filesystems: &Path) -> bool {
    fs::read_to_string(filesystems)
        .unwrap_or_default()
        .lines()
        .any(|line| line.split_whitespace().last() == Some(MECHANISM_TOOLBOX_FILESYSTEM))
}

/// Whether `dir`'s file system opens a file for direct I/O, which is what the
/// loop device behind a workspace disk needs to skip the host's page cache.
/// An unnamed file, so the probe leaves nothing behind.
#[cfg(target_os = "linux")]
#[must_use]
pub fn direct_io(dir: &Path) -> bool {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::open(
        dir,
        OFlags::TMPFILE | OFlags::RDWR | OFlags::DIRECT,
        Mode::RUSR | Mode::WUSR,
    )
    .is_ok()
}

/// No other kernel's loop device skips the page cache.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn direct_io(_dir: &Path) -> bool {
    false
}
