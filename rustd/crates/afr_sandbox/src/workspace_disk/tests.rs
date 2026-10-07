#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::PathBuf;

use std::os::unix::fs::PermissionsExt as _;

use super::{TMP_DIR, WORKSPACE_DIR, WorkspaceDisk, absent, lay_out};
use crate::host::HostTools;

/// Tools that cannot run, so every build stops at a known step.
fn broken(mke2fs: &str) -> HostTools {
    HostTools {
        bwrap: PathBuf::from("/nonexistent/bwrap"),
        mke2fs: PathBuf::from(mke2fs),
        mount: PathBuf::from("/nonexistent/mount"),
    }
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
    let refused = WorkspaceDisk::create(&broken("/usr/bin/true"), dir.path(), 4_096, (0, 0)).await;

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
    // A non-empty directory where the mount point goes cannot be removed.
    fs::create_dir_all(dir.path().join("workspace").join("held")).unwrap();

    let refused = WorkspaceDisk::create(&broken("/usr/bin/true"), dir.path(), 4_096, (0, 0)).await;

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
    let me = (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    );

    lay_out(root.path(), me).unwrap();

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

/// A disk whose layout is refused once it is mounted is undone as a release
/// undoes one — unmounted where anything is mounted, then removed — so the
/// build leaves nothing behind, and the caller sees the layout's own failure.
/// The fake mount makes no disk, so the layout lands in the bare mount point.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_a_disk_that_cannot_be_laid_out_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let tools = HostTools {
        mount: PathBuf::from("/usr/bin/true"),
        ..broken("/usr/bin/true")
    };

    // Owned by a user this unprivileged test is not, so the layout's chown
    // is refused once `workspace/` is made.
    let refused = WorkspaceDisk::create(&tools, dir.path(), 4_096, (65_534, 65_534)).await;

    let failure = refused.err().map(|error| {
        std::error::Error::source(&error)
            .and_then(|cause| cause.downcast_ref::<std::io::Error>())
            .map(std::io::Error::kind)
    });
    assert_eq!(
        failure,
        Some(Some(std::io::ErrorKind::PermissionDenied)),
        "the layout's own failure reaches the caller"
    );
    assert_eq!(
        fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        Vec::<std::ffi::OsString>::new(),
        "image and mount point removed"
    );
}
