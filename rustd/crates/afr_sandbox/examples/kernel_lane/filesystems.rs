//! What the host's file systems make of a lease: whether the probe and the
//! workspace disk find direct I/O, whether a disk its mount helper mounted
//! and then failed is undone, and whether the lane's own disk check finds
//! room. Each trial mounts a file system of its own to stand for a host whose
//! state lives somewhere else than the lane's.

use std::ffi::{CStr, CString};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use afr_sandbox::{Caching, HostTools, MOUNT_PATH, ProbePaths, WorkspaceDisk, probe};
use libtest_mimic::Failed;
use rustix::mount::{MountFlags, UnmountFlags};

use crate::admission::MOUNTINFO;
use crate::lane::{
    ENTRY_MODE, Lane, SANDBOX_IDS, STATE_FREE_BYTES_MIN, STATE_PREFIX, short_of_disk,
};
use crate::run::{expect, runtime};
use crate::trials::SMALL_DISK;

/// A file system that makes unnamed files but refuses direct I/O: what the
/// probe and the loop device both meet on a host whose state is not on a disk.
const RAMFS: &str = "ramfs";
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
/// A mount helper that mounts as the real one does, then fails: what the
/// workspace disk's undo meets when a helper dies after its mount.
const MOUNTS_THEN_FAILS: &str = "mounts-then-fails";
/// The file the helper makes once its mount succeeded, beside the lease.
const MOUNTED_MARK: &str = "mounted-then-failed";
/// Where each trial's file system is mounted, under its own scratch directory.
const MOUNTED: &str = "mounted";
/// The lease directory a workspace disk is made in.
const LEASE: &str = "lease";

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
        .ok_or_else(|| Failed::from("the leases live under the lane's state"))
}

/// How a workspace disk made in a fresh lease directory under `parent` is
/// cached; the disk is released before this returns.
fn caching_under(parent: &Path) -> Result<Caching, Failed> {
    let lease = tempfile::tempdir_in(parent)?;
    let dir = lease.path().join(LEASE);
    fs::create_dir(&dir)?;
    let (disk, caching) = runtime().block_on(WorkspaceDisk::create(
        &HostTools::default(),
        &dir,
        SMALL_DISK,
        SANDBOX_IDS,
    ))?;
    disk.release()?;
    Ok(caching)
}

/// A workspace disk whose image sits on a file system that refuses direct
/// I/O still builds: its loop device stays on the host's page cache, and the
/// caller is told so. On the lane's own disk the same build reads direct.
pub(crate) fn buffered_disk(lane: &Lane) -> Result<(), Failed> {
    let ramfs = Mounted::new(on_disk(lane)?, RAMFS, None)?;

    let refused = caching_under(&ramfs.point)?;
    let allowed = caching_under(on_disk(lane)?)?;

    expect(
        refused == Caching::Buffered,
        format!("a ramfs image runs buffered, got {refused:?}"),
    )?;
    expect(
        allowed == Caching::Direct,
        format!("an image on disk runs direct, got {allowed:?}"),
    )
}

/// A disk whose mount helper mounted it and then failed is unmounted before
/// its files go: the build fails, and leaves nothing in its directory and
/// nothing mounted, so no loop device stays on a deleted image.
pub(crate) fn failed_mount_is_unmounted(lane: &Lane) -> Result<(), Failed> {
    let lease = tempfile::tempdir_in(on_disk(lane)?)?;
    let helper = lease.path().join(MOUNTS_THEN_FAILS);
    let mark = lease.path().join(MOUNTED_MARK);
    fs::write(
        &helper,
        format!(
            "#!/bin/sh\n{MOUNT_PATH} \"$@\" && : > '{}'\nexit 1\n",
            mark.display()
        ),
    )?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(ENTRY_MODE))?;
    let dir = lease.path().join(LEASE);
    fs::create_dir(&dir)?;
    let tools = HostTools {
        mount: helper,
        ..HostTools::default()
    };

    let built = runtime().block_on(WorkspaceDisk::create(&tools, &dir, SMALL_DISK, SANDBOX_IDS));

    let refused = match built {
        Ok((disk, _caching)) => disk.release().map(|()| false)?,
        Err(_failed) => true,
    };
    let left: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    let mounts = fs::read_to_string(MOUNTINFO).unwrap_or_default();
    expect(mark.exists(), "the helper mounted before it failed")?;
    expect(refused, "the helper's failure fails the build")?;
    expect(
        left.is_empty(),
        format!("the lease's directory is left empty, got {left:?}"),
    )?;
    expect(
        !mounts.contains(&dir.display().to_string()),
        "nothing stays mounted",
    )
}

/// The probe says whether a state directory's file system takes direct I/O:
/// yes on the lane's disk, no on a file system that refuses it.
pub(crate) fn probe_direct_io(lane: &Lane) -> Result<(), Failed> {
    let ramfs = Mounted::new(on_disk(lane)?, RAMFS, None)?;
    let probed = |state_dir: &Path| {
        probe(&ProbePaths {
            state_dir: Some(state_dir.to_owned()),
            ..ProbePaths::default()
        })
        .workspace_direct_io
    };

    let (disk, memory) = (probed(on_disk(lane)?), probed(&ramfs.point));

    expect(
        disk == Some(true),
        format!("the lane's disk takes direct I/O, got {disk:?}"),
    )?;
    expect(
        memory == Some(false),
        format!("ramfs refuses it, got {memory:?}"),
    )
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
    let unreadable = short_of_disk(&small.point.join(LEASE)).unwrap_or_default();

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
