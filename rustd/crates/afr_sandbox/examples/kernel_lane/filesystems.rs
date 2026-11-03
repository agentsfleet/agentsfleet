//! What the host's file systems make of a lease: whether the probe and the
//! workspace disk find direct I/O, whether a disk its mount helper mounted
//! and then failed is undone, and whether the lane's own disk check finds
//! room. Each trial mounts a file system of its own to stand for a host whose
//! state lives somewhere else than the lane's.

use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};

use afd_core::test_util::trace::Capture;
use afr_sandbox::{Caching, HostTools, ProbePaths, WorkspaceDisk, probe};
use libtest_mimic::Failed;
use rustix::mount::{MountFlags, UnmountFlags};

use crate::admission::MOUNTINFO;
use crate::lane::{Lane, SANDBOX_IDS, STATE_FREE_BYTES_MIN, STATE_PREFIX, short_of_disk};
use crate::run::{expect, runtime};
use crate::trials::{Body, SMALL_DISK};

/// What the workspace disk leaves behind and what its build starts.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    (
        "test_refused_mount_leaves_nothing",
        refused_mount_leaves_nothing,
    ),
    (
        "test_released_disk_leaves_no_loop_device",
        released_disk_leaves_no_loop_device,
    ),
    (
        "test_workspace_disk_starts_no_mount_program",
        disk_starts_no_mount_program,
    ),
];

/// A file system that makes unnamed files but refuses direct I/O: what the
/// probe and the loop device both meet on a host whose state is not on a disk.
const RAMFS: &str = "ramfs";
/// The event every host program's start is logged under, and the one
/// program a disk's build may start.
const PROGRAM_STARTED: &str = "sandbox_host_program_started";
const MKE2FS: &str = "mke2fs";
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
/// A formatter that formats nothing, so the mount meets no file system.
const FORMATS_NOTHING: &str = "/usr/bin/true";
/// Where the kernel lists block devices, and the file naming a loop device's
/// backing file.
const SYS_BLOCK: &str = "/sys/block";
const BACKING_FILE: &str = "loop/backing_file";
/// How long a loop device's detach may take to finish, and how often it is
/// looked at.
const DETACH_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
const DETACH_POLL: std::time::Duration = std::time::Duration::from_millis(10);
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

/// Building a lease's disk starts `mke2fs` and
/// no other host program; the loop device and the mount are made in-process.
pub(crate) fn disk_starts_no_mount_program(lane: &Lane) -> Result<(), Failed> {
    let lease = tempfile::tempdir_in(on_disk(lane)?)?;
    let dir = lease.path().join(LEASE);
    fs::create_dir(&dir)?;

    let capture = Capture::install();
    let built = runtime().block_on(WorkspaceDisk::create(
        &HostTools::default(),
        &dir,
        SMALL_DISK,
        SANDBOX_IDS,
    ));
    let started: Vec<String> = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(PROGRAM_STARTED))
        .filter_map(|event| event.field("program").map(str::to_owned))
        .collect();
    drop(capture);
    built?.0.release()?;

    expect(
        started == [MKE2FS],
        format!("the disk's build started only mke2fs, got {started:?}"),
    )
}

/// A disk the kernel will not mount, an image `mke2fs` never formatted,
/// fails the build and leaves nothing: no file in its directory, nothing
/// mounted, and no loop device on the image.
pub(crate) fn refused_mount_leaves_nothing(lane: &Lane) -> Result<(), Failed> {
    let lease = tempfile::tempdir_in(on_disk(lane)?)?;
    let dir = lease.path().join(LEASE);
    fs::create_dir(&dir)?;
    let tools = HostTools {
        mke2fs: PathBuf::from(FORMATS_NOTHING),
        ..HostTools::default()
    };

    let built = runtime().block_on(WorkspaceDisk::create(&tools, &dir, SMALL_DISK, SANDBOX_IDS));
    let refused = match built {
        Ok((disk, _caching)) => {
            disk.release()?;
            None
        }
        Err(refused) => Some(refused),
    };

    let left: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    let mounts = fs::read_to_string(MOUNTINFO).unwrap_or_default();
    // The mount's own refusal, a system call's `EINVAL`: an attach that failed
    // first is an input/output failure, and would leave no loop device to free.
    let cause = refused
        .as_ref()
        .and_then(|refused| std::error::Error::source(refused))
        .and_then(|cause| cause.downcast_ref::<rustix::io::Errno>())
        .copied();
    expect(
        cause == Some(rustix::io::Errno::INVAL),
        format!(
            "the mount refused an image with no file system, got {:?}",
            refused.as_ref().map(afr_sandbox::Error::told)
        ),
    )?;
    expect(
        left.is_empty(),
        format!("the lease's directory is left empty, got {left:?}"),
    )?;
    expect(
        !mounts.contains(&dir.display().to_string()),
        "nothing stays mounted",
    )?;
    let loops = settled_loops_on(&dir)?;
    expect(
        loops.is_empty(),
        format!("no loop device stays, got {loops:?}"),
    )
}

/// A disk the build made has a loop device of its own, and its release
/// frees it: autoclear detaches the device once the unmount lets go.
pub(crate) fn released_disk_leaves_no_loop_device(lane: &Lane) -> Result<(), Failed> {
    let lease = tempfile::tempdir_in(on_disk(lane)?)?;
    let dir = lease.path().join(LEASE);
    fs::create_dir(&dir)?;
    let tools = HostTools::default();

    let (disk, _caching) =
        runtime().block_on(WorkspaceDisk::create(&tools, &dir, SMALL_DISK, SANDBOX_IDS))?;
    let attached = loops_on(&dir)?;
    disk.release()?;
    let left = settled_loops_on(&dir)?;

    expect(
        attached.len() == 1,
        format!("the disk had one loop device, got {attached:?}"),
    )?;
    expect(
        left.is_empty(),
        format!("the release freed it, got {left:?}"),
    )
}

/// The loop devices whose backing file lies under `dir`.
fn loops_on(dir: &Path) -> Result<Vec<String>, Failed> {
    let under = dir.display().to_string();
    let mut named = Vec::new();
    for device in fs::read_dir(SYS_BLOCK)?.flatten() {
        let backing = fs::read_to_string(device.path().join(BACKING_FILE)).unwrap_or_default();
        if backing.trim_end().starts_with(&under) {
            named.push(device.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(named)
}

/// [`loops_on`], once the kernel has finished any detach it started: an
/// autoclear may complete just after the last close returns.
fn settled_loops_on(dir: &Path) -> Result<Vec<String>, Failed> {
    let deadline = std::time::Instant::now() + DETACH_WAIT;
    loop {
        let loops = loops_on(dir)?;
        if loops.is_empty() || std::time::Instant::now() >= deadline {
            return Ok(loops);
        }
        std::thread::sleep(DETACH_POLL);
    }
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
