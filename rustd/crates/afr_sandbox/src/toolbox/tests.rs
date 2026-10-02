#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::{Path, PathBuf};

use super::{Toolbox, ToolboxImage, is_mount_point, sha256_of};
use crate::host::HostTools;

/// The SHA-256 of `toolbox bytes`, so an image can be named correctly.
const BYTES: &[u8] = b"toolbox bytes";

fn named(dir: &Path, digest: &str) -> PathBuf {
    let path = dir.join(format!("toolbox-{digest}.erofs"));
    fs::write(&path, BYTES).unwrap();
    path
}

#[test]
fn test_an_image_whose_bytes_match_its_name_is_verified() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::write(&scratch, BYTES).unwrap();
    let digest = sha256_of(&scratch).unwrap();

    let image = ToolboxImage::verify(&named(dir.path(), &digest)).unwrap();

    assert_eq!(image.digest(), digest);
    assert!(image.path().ends_with(format!("toolbox-{digest}.erofs")));
}

/// A flipped byte is never mounted: [`Toolbox::mount`] takes only a
/// [`ToolboxImage`], and `verify` is the only way to make one.
#[test]
fn test_toolbox_hash_mismatch_refused() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::write(&scratch, BYTES).unwrap();
    let digest = sha256_of(&scratch).unwrap();
    let path = named(dir.path(), &digest);
    let mut tampered = BYTES.to_vec();
    if let Some(first) = tampered.first_mut() {
        *first ^= 1;
    }
    fs::write(&path, &tampered).unwrap();
    fs::write(&scratch, &tampered).unwrap();
    let actual = sha256_of(&scratch).unwrap();

    let refused = ToolboxImage::verify(&path).unwrap_err().to_string();

    assert_ne!(actual, digest);
    assert!(
        refused.contains(&format!("(it hashes to {actual})")),
        "{refused}"
    );
}

#[test]
fn test_an_image_named_without_a_digest_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rootfs.img");
    fs::write(&path, BYTES).unwrap();

    ToolboxImage::verify(&path).unwrap_err();
    ToolboxImage::verify(&dir.path().join("absent.erofs")).unwrap_err();
}

#[test]
fn test_mountinfo_names_the_mount_point_field() {
    let mountinfo = "36 35 98:0 / /mnt/toolbox/abc ro,relatime - erofs /dev/loop0 ro\n\
                     37 35 98:1 /mnt/toolbox/abc /elsewhere rw - ext4 /dev/vda rw\n";

    assert!(is_mount_point(mountinfo, Path::new("/mnt/toolbox/abc")));
    assert!(!is_mount_point(mountinfo, Path::new("/mnt/toolbox")));
    assert!(!is_mount_point("", Path::new("/mnt/toolbox/abc")));
}

#[tokio::test]
async fn test_a_toolbox_that_cannot_be_mounted_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::write(&scratch, BYTES).unwrap();
    let image = ToolboxImage::verify(&named(dir.path(), &sha256_of(&scratch).unwrap())).unwrap();
    let tools = HostTools {
        mount: PathBuf::from("/nonexistent/mount"),
        ..HostTools::default()
    };

    let refused = Toolbox::mount(&image, &tools, &dir.path().join("mounts")).await;

    refused.unwrap_err();
    assert!(
        dir.path().join("mounts").join(image.digest()).is_dir(),
        "the mount point was made"
    );
}

#[test]
fn test_an_adopted_root_is_used_as_given() {
    assert_eq!(Toolbox::at(PathBuf::from("/")).root(), Path::new("/"));
}

#[tokio::test]
async fn test_a_toolbox_mounts_once_and_is_adopted_after() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::write(&scratch, BYTES).unwrap();
    let image = ToolboxImage::verify(&named(dir.path(), &sha256_of(&scratch).unwrap())).unwrap();
    // `true` stands in for `mount`, so the mount point is made and "mounted".
    let tools = HostTools {
        mount: PathBuf::from("/usr/bin/true"),
        ..HostTools::default()
    };

    let toolbox = Toolbox::mount(&image, &tools, &dir.path().join("mounts"))
        .await
        .unwrap();

    assert_eq!(
        toolbox.root(),
        dir.path().join("mounts").join(image.digest())
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_toolbox_that_is_not_mounted_cannot_be_unmounted() {
    let dir = tempfile::tempdir().unwrap();

    let refused = Toolbox::at(dir.path().to_owned()).unmount();

    refused.unwrap_err();
    assert!(
        dir.path().exists(),
        "nothing is removed while still mounted"
    );
}
