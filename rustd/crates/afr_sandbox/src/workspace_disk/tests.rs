#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::{WorkspaceDisk, absent, format_arguments, mount_arguments};
use crate::host::HostTools;

fn strings(arguments: Vec<OsString>) -> Vec<String> {
    arguments
        .into_iter()
        .map(|part| part.into_string().unwrap())
        .collect()
}

/// The owner a lease's workspace disk is formatted for.
const USER: u32 = 1000;
/// Its group.
const GROUP: u32 = 100;

#[test]
fn test_format_arguments_make_a_journal_free_disk_its_owner_may_write() {
    let arguments = strings(format_arguments(
        Path::new("/s/workspace.img"),
        (USER, GROUP),
    ));

    assert_eq!(
        arguments,
        [
            "-q",
            "-F",
            "-t",
            "ext4",
            "-m",
            "0",
            "-O",
            "^has_journal",
            "-E",
            // pin test: literal is the contract
            "root_owner=1000:100",
            "/s/workspace.img",
        ]
    );
}

#[test]
fn test_mount_arguments_name_type_options_source_and_target() {
    let arguments = strings(mount_arguments(
        "ext4",
        "loop,nosuid,nodev",
        Path::new("/a"),
        Path::new("/b"),
    ));

    assert_eq!(
        arguments,
        ["-t", "ext4", "-o", "loop,nosuid,nodev", "/a", "/b"]
    );
}

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
