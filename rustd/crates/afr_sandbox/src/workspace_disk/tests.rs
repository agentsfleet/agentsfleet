#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::time::Duration;

use super::{Caching, Mount, TMP_DIR, WORKSPACE_DIR, WorkspaceDisk, absent, lay_out};
use crate::host::HostTools;

/// A program that does nothing and succeeds: a format that made nothing.
const SUCCEEDS: &str = "/usr/bin/true";
/// How long a slow mount takes, against the start's far shorter wait.
const SLOW_MOUNT: Duration = Duration::from_millis(200);
/// How long a test waits for the blocking pool to undo a mount.
const PATIENCE: Duration = Duration::from_secs(5);
/// Tools that cannot run, so every build stops at a known step.
fn broken(mke2fs: &str) -> HostTools {
    HostTools {
        bwrap: PathBuf::from("/nonexistent/bwrap"),
        mke2fs: PathBuf::from(mke2fs),
        ..HostTools::default()
    }
}

/// Tools that format nothing and mount nothing, both successfully.
fn faked(mount: Mount) -> HostTools {
    HostTools {
        mount,
        ..broken(SUCCEEDS)
    }
}

/// The test's own user and group, whom every chown is granted to.
fn me() -> (u32, u32) {
    (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    )
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

    // `true` formats nothing and succeeds, so the build reaches the mount,
    // which the kernel refuses: no file system, no loop device, or no root.
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

#[tokio::test]
async fn test_a_heard_mount_is_laid_out_and_kept() {
    let dir = tempfile::tempdir().unwrap();

    let (disk, caching) = WorkspaceDisk::create(
        &faked(|_image, _mount_point| Ok(Caching::Direct)),
        dir.path(),
        4_096,
        me(),
    )
    .await
    .unwrap();

    assert_eq!(caching, Caching::Direct);
    assert!(disk.workspace().is_dir() && disk.tmp().is_dir());
    assert!(
        dir.path().join("workspace.img").exists(),
        "kept for release"
    );
}

/// A start dropped while the blocking pool mounts leaves nobody to release
/// the disk, so the mount undoes itself once it lands.
#[tokio::test]
async fn test_a_mount_nobody_heard_is_undone() {
    let dir = tempfile::tempdir().unwrap();

    let dropped = tokio::time::timeout(
        SLOW_MOUNT / 10,
        WorkspaceDisk::create(
            &faked(|_image, _mount_point| {
                std::thread::sleep(SLOW_MOUNT);
                Ok(Caching::Direct)
            }),
            dir.path(),
            4_096,
            me(),
        ),
    )
    .await;

    dropped.unwrap_err();
    assert!(dir.path().join("workspace").exists(), "still mounting");
    tokio::time::timeout(PATIENCE, async {
        while fs::read_dir(dir.path()).unwrap().count() > 0 {
            tokio::time::sleep(SLOW_MOUNT / 10).await;
        }
    })
    .await
    .unwrap();
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

/// Each caching names itself on the admission and disk logs.
#[test]
fn test_each_caching_names_itself() {
    assert_eq!(Caching::Direct.as_str(), "direct");
    assert_eq!(Caching::Buffered.as_str(), "buffered");
}
