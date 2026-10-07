//! What the host's file systems make of a lease: whether the probe and the
//! workspace disk find direct I/O, and whether the lane's own disk check
//! finds room. Each trial mounts a file system of its own to stand for a host
//! whose state lives somewhere else than the lane's.

use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};

use afr_sandbox::{Caching, HostTools, ProbePaths, WorkspaceDisk, probe};
use libtest_mimic::Failed;
use rustix::mount::{MountFlags, UnmountFlags};

use crate::lane::{Lane, SANDBOX_IDS, STATE_PREFIX, short_of_disk};
use crate::run::{expect, runtime};
use crate::trials::SMALL_DISK;

/// A file system that makes unnamed files but refuses direct I/O: what the
/// probe and the loop device both meet on a host whose state is not on a disk.
const RAMFS: &str = "ramfs";
/// A small memory file system, far short of what a lane run fills.
const TMPFS: &str = "tmpfs";
const SMALL_TMPFS: &CStr = c"size=16m";
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
/// state an earlier run left there; passes the disk the lane runs on; and
/// names a parent it cannot read.
pub(crate) fn short_disk(lane: &Lane) -> Result<(), Failed> {
    let small = Mounted::new(on_disk(lane)?, TMPFS, Some(SMALL_TMPFS))?;
    let earlier = format!("{STATE_PREFIX}earlier");
    fs::create_dir(small.point.join(&earlier))?;

    let short = short_of_disk(&small.point).unwrap_or_default();
    let roomy = short_of_disk(on_disk(lane)?);
    let unreadable = short_of_disk(&small.point.join(LEASE)).unwrap_or_default();

    expect(
        short.contains("MiB free") && short.contains(&earlier),
        format!("short of room, naming {earlier}, got {short:?}"),
    )?;
    expect(
        roomy.is_none(),
        format!("the lane's disk has room, got {roomy:?}"),
    )?;
    expect(
        unreadable.contains("is unreadable"),
        format!("an absent parent is named, got {unreadable:?}"),
    )
}
