#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tempfile::TempDir;

use super::{Caching, IMAGE_MODE, TMP_DIR, WORKSPACE_DIR, WorkspaceDisk, absent, lay_out};
use crate::host::HostTools;

/// A program that does nothing and succeeds: a format or mount that made
/// nothing.
const SUCCEEDS: &str = "/usr/bin/true";
/// A fake `mount` that writes into the mount point, its last argument, and
/// then fails, as a helper that mounted before it failed leaves the disk.
const MOUNTS_THEN_FAILS: &str = "mounts-then-fails";
/// A fake `mount` that leaves a `workspace/` in the mount point and succeeds,
/// so the layout's own is refused whoever runs the test.
const LEAVES_WORKSPACE: &str = "leaves-workspace";
/// A fake helper's mode: it runs.
const HELPER_MODE: u32 = 0o755;
/// The permission bits of a file's mode.
const PERMISSION_BITS: u32 = 0o7777;
/// The size a test disk asks for.
const DISK_BYTES: u64 = 4_096;
/// Shell that sets `target` to a helper's last argument, the mount point.
const LAST_ARGUMENT: &str = "for target; do :; done";

/// Tools that cannot run, so every build stops at a known step.
fn broken(mke2fs: &str) -> HostTools {
    HostTools {
        bwrap: PathBuf::from("/nonexistent/bwrap"),
        mke2fs: PathBuf::from(mke2fs),
        mount: PathBuf::from("/nonexistent/mount"),
    }
}

/// Tools that format nothing and mount with the fake helper `name`.
fn mounting_with(name: &str) -> HostTools {
    HostTools {
        mount: helpers().join(name),
        ..broken(SUCCEEDS)
    }
}

/// The fake mount helpers, written once per test process so no test
/// executes a file another thread is still writing.
fn helpers() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [
            (
                MOUNTS_THEN_FAILS,
                format!("#!/bin/sh\n{LAST_ARGUMENT}\nmkdir \"$target/lost+found\"\nexit 1\n"),
            ),
            (
                LEAVES_WORKSPACE,
                format!("#!/bin/sh\n{LAST_ARGUMENT}\nmkdir \"$target/{WORKSPACE_DIR}\"\n"),
            ),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(HELPER_MODE)).unwrap();
        }
        dir
    })
    .path()
}

/// The test's own user and group, whom every chown is granted to.
fn me() -> (u32, u32) {
    (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    )
}

/// What `dir` holds, by name.
fn entries(dir: &Path) -> Vec<OsString> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

#[tokio::test]
async fn test_a_disk_that_cannot_be_formatted_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();

    let refused =
        WorkspaceDisk::create(&broken("/nonexistent/mke2fs"), dir.path(), 4_096, (0, 0)).await;

    refused.unwrap_err();
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        0,
        "image removed"
    );
}

#[tokio::test]
async fn test_a_disk_that_cannot_be_mounted_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();

    // `true` formats nothing and succeeds, so the build reaches the mount.
    let refused = WorkspaceDisk::create(&broken(SUCCEEDS), dir.path(), 4_096, (0, 0)).await;

    refused.unwrap_err();
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        0,
        "image and mount point removed"
    );
}

#[tokio::test]
async fn test_a_leftover_that_cannot_be_removed_is_logged() {
    let dir = tempfile::tempdir().unwrap();
    let capture = afd_core::test_util::trace::Capture::install();
    // A file where the mount point goes is no directory to remove, whoever
    // runs the test.
    fs::write(dir.path().join(WORKSPACE_DIR), "").unwrap();

    let refused = WorkspaceDisk::create(&broken(SUCCEEDS), dir.path(), 4_096, (0, 0)).await;

    refused.unwrap_err();
    assert_eq!(
        capture.only("sandbox_workspace_left").field("event"),
        Some("sandbox_workspace_left")
    );
}

#[test]
fn test_absent_forgives_only_a_missing_file() {
    absent(std::io::ErrorKind::NotFound.into()).unwrap();
    absent(std::io::ErrorKind::PermissionDenied.into()).unwrap_err();
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_disk_that_will_not_unmount_keeps_its_image() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("workspace.img"), "").unwrap();
    fs::create_dir(dir.path().join("workspace")).unwrap();

    // Nothing is mounted there, so the kernel refuses the unmount.
    WorkspaceDisk::leftover(dir.path()).release().unwrap_err();

    assert!(dir.path().join("workspace.img").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_leftover_disk_with_nothing_mounted_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("workspace.img"), "").unwrap();
    fs::create_dir(dir.path().join("workspace")).unwrap();

    WorkspaceDisk::leftover(dir.path())
        .release_leftover()
        .unwrap();
    WorkspaceDisk::leftover(dir.path())
        .release_leftover()
        .unwrap();

    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        0,
        "and twice is no failure"
    );
}

/// The disk is laid out as `workspace/` (0755) and `tmp/` (1777), owned by
/// the sandbox user, whatever the umask: the sticky bit is what keeps one
/// tenant process from removing another's scratch file.
#[test]
fn test_the_disk_is_laid_out_as_workspace_and_sticky_tmp() {
    let root = tempfile::tempdir().unwrap();

    lay_out(root.path(), me()).unwrap();

    let mode = |name: &str| {
        fs::metadata(root.path().join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    assert_eq!(mode(WORKSPACE_DIR), 0o755);
    assert_eq!(mode(TMP_DIR), 0o1777);
    let disk = WorkspaceDisk::in_dir(root.path());
    assert!(disk.workspace().ends_with("workspace/workspace"));
    assert!(disk.tmp().ends_with("workspace/tmp"));
}

/// Switching a device to direct I/O falls back to buffered only when the
/// backing file system refuses it; refused for any other reason, it is a
/// failure. `/dev/null` is no loop device, so the kernel answers `ENOTTY`.
#[cfg(target_os = "linux")]
#[test]
fn test_a_direct_io_switch_refused_for_another_reason_is_an_error() {
    let switched = crate::toolbox::loop_device::direct_io(std::path::Path::new("/dev/null"));

    assert!(switched.is_err(), "{switched:?}");
}

/// A mount helper that mounted and then failed leaves a disk the undo must
/// unmount before its files go, as a release does: the build leaves nothing
/// behind. The fake mounts nothing, so what it wrote lands in the bare mount
/// point, which goes with its contents.
#[tokio::test]
async fn test_a_mount_that_fails_after_mounting_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();

    let refused =
        WorkspaceDisk::create(&mounting_with(MOUNTS_THEN_FAILS), dir.path(), 4_096, me()).await;

    refused.unwrap_err();
    assert_eq!(
        entries(dir.path()),
        Vec::<OsString>::new(),
        "image and mount point removed"
    );
}

/// A disk whose layout is refused once it is mounted is undone as a release
/// undoes one — unmounted where anything is mounted, then removed with what
/// the layout made — so the build leaves nothing behind, and the caller sees
/// the layout's own failure. The fake mount makes no disk and leaves a
/// `workspace/` in the bare mount point, so the layout's is refused whoever
/// runs the test.
#[tokio::test]
async fn test_a_disk_that_cannot_be_laid_out_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();

    let refused =
        WorkspaceDisk::create(&mounting_with(LEAVES_WORKSPACE), dir.path(), 4_096, me()).await;

    let failure = refused.err().map(|error| {
        std::error::Error::source(&error)
            .and_then(|cause| cause.downcast_ref::<std::io::Error>())
            .map(std::io::Error::kind)
    });
    assert_eq!(
        failure,
        Some(Some(std::io::ErrorKind::AlreadyExists)),
        "the layout's own failure reaches the caller"
    );
    assert_eq!(
        entries(dir.path()),
        Vec::<OsString>::new(),
        "image and mount point removed"
    );
}

/// A build whose format and mount succeed hands back the disk: its image is
/// root's alone and sized to the limit, `workspace/` and `tmp/` are laid out
/// on the mount, and a mount that made no loop device is cached buffered.
/// The fakes format and mount nothing, so the layout lands in the bare mount
/// point.
#[tokio::test]
async fn test_a_disk_that_builds_is_laid_out_and_cached_buffered() {
    let dir = tempfile::tempdir().unwrap();
    let tools = HostTools {
        mount: PathBuf::from(SUCCEEDS),
        ..broken(SUCCEEDS)
    };

    let (disk, caching) = WorkspaceDisk::create(&tools, dir.path(), DISK_BYTES, me())
        .await
        .unwrap();

    assert_eq!(caching, Caching::Buffered);
    let image = fs::metadata(dir.path().join(super::IMAGE_NAME)).unwrap();
    assert_eq!(image.len(), DISK_BYTES);
    assert_eq!(image.permissions().mode() & PERMISSION_BITS, IMAGE_MODE);
    assert_eq!(disk.mount_point(), dir.path().join(super::MOUNT_DIR));
    let tmp = fs::metadata(disk.tmp()).unwrap().permissions().mode();
    assert_eq!(tmp & PERMISSION_BITS, super::TMP_MODE);
    assert!(disk.workspace().is_dir());
}
