#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::PathBuf;

use super::{WorkspaceDisk, absent};
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
