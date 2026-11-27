//! Whether the lane's own disk check finds room: each case mounts a file
//! system of its own to stand for a host whose state lives somewhere else
//! than the lane's.

use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};

use libtest_mimic::Failed;
use rustix::mount::{MountFlags, UnmountFlags};

use crate::lane::{Lane, STATE_FREE_BYTES_MIN, STATE_PREFIX, short_of_disk};
use crate::run::expect;

/// A small memory file system, far short of what a lane run fills.
const TMPFS: &str = "tmpfs";
const SMALL_TMPFS_BYTES: u64 = 16 << 20;
/// A tmpfs past the lane's free-space floor. Its size is a limit, not memory
/// taken, so the check reads room whatever the host has left.
const ROOMY_TMPFS_BYTES: u64 = 8 << 30;
const _: () = assert!(
    ROOMY_TMPFS_BYTES > STATE_FREE_BYTES_MIN,
    "a roomy tmpfs must clear the lane's free-space floor"
);
/// Where each trial's file system is mounted, under its own scratch directory.
const MOUNTED: &str = "mounted";
/// A directory no trial makes, so the check cannot read it.
const ABSENT: &str = "absent";

/// A file system mounted for one trial, unmounted when the trial ends, pass
/// or fail.
struct Mounted {
    point: PathBuf,
    _scratch: tempfile::TempDir,
}

impl Mounted {
    /// Mounts `fstype` with `data` on a fresh directory under `parent`.
    fn new(parent: &Path, fstype: &str, data: Option<&CStr>) -> Result<Self, Failed> {
        let scratch = tempfile::tempdir_in(parent)?;
        let point = scratch.path().join(MOUNTED);
        fs::create_dir(&point)?;
        rustix::mount::mount(fstype, &point, fstype, MountFlags::empty(), data)?;
        Ok(Self {
            point,
            _scratch: scratch,
        })
    }
}

impl Drop for Mounted {
    fn drop(&mut self) {
        let _unmounted = rustix::mount::unmount(&self.point, UnmountFlags::DETACH);
    }
}

/// A tmpfs's mount data, limiting it to `bytes`.
fn tmpfs_size(bytes: u64) -> Result<CString, Failed> {
    Ok(CString::new(format!("size={bytes}"))?)
}

/// The lane's state directory, on the disk the lane runs on.
fn on_disk(lane: &Lane) -> Result<&Path, Failed> {
    lane.config
        .state_dir
        .parent()
        .ok_or_else(|| Failed::from("the runs live under the lane's state"))
}

/// The lane's disk check refuses a parent short of room, naming the lane
/// state an earlier run left there; passes a parent with room; and names a
/// parent it cannot read. Both sizes are tmpfs limits, so the trial never
/// depends on what the host has free.
pub(crate) fn short_disk(lane: &Lane) -> Result<(), Failed> {
    let small = Mounted::new(on_disk(lane)?, TMPFS, Some(&tmpfs_size(SMALL_TMPFS_BYTES)?))?;
    let earlier = format!("{STATE_PREFIX}earlier");
    fs::create_dir(small.point.join(&earlier))?;

    let short = short_of_disk(&small.point).unwrap_or_default();
    let roomy_fs = Mounted::new(on_disk(lane)?, TMPFS, Some(&tmpfs_size(ROOMY_TMPFS_BYTES)?))?;
    let roomy = short_of_disk(&roomy_fs.point);
    let unreadable = short_of_disk(&small.point.join(ABSENT)).unwrap_or_default();

    expect(
        short.contains("MiB free") && short.contains(&earlier),
        format!("short of room, naming {earlier}, got {short:?}"),
    )?;
    expect(
        roomy.is_none(),
        format!("a parent with room passes, got {roomy:?}"),
    )?;
    expect(
        unreadable.contains("is unreadable"),
        format!("an absent parent is named, got {unreadable:?}"),
    )
}
