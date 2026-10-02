#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::{Path, PathBuf};

use super::{Toolbox, ToolboxImage, sha256_of};
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
    let toolbox = Toolbox::at(PathBuf::from("/"), "abc".to_owned());

    assert_eq!((toolbox.root(), toolbox.digest()), (Path::new("/"), "abc"));
}

/// A mount that reports success but leaves no loop device of the image behind
/// is refused: what is verified is what is mounted, not what was asked for.
#[tokio::test]
async fn test_a_mount_that_is_not_the_image_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("scratch");
    fs::write(&scratch, BYTES).unwrap();
    let image = ToolboxImage::verify(&named(dir.path(), &sha256_of(&scratch).unwrap())).unwrap();
    // `true` stands in for `mount`: it succeeds and mounts nothing.
    let tools = HostTools {
        mount: PathBuf::from("/usr/bin/true"),
        ..HostTools::default()
    };
    let capture = afd_core::test_util::trace::Capture::install();

    let refused = Toolbox::mount(&image, &tools, &dir.path().join("mounts")).await;

    refused.unwrap_err();
    // Logged with the digest it was for; on Linux, the attempt to detach what
    // was mounted at the root is logged beside it.
    let failed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("sandbox_toolbox_mount_failed"))
        .collect();
    assert!(
        failed
            .iter()
            .any(|event| event.field("digest") == Some(image.digest())
                && event.field("error_code").is_some()),
        "{failed:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_toolbox_that_is_not_mounted_cannot_be_unmounted() {
    let dir = tempfile::tempdir().unwrap();

    let refused = Toolbox::at(dir.path().to_owned(), "abc".to_owned()).unmount();

    refused.unwrap_err();
    assert!(
        dir.path().exists(),
        "nothing is removed while still mounted"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_only_a_loop_device_is_named_and_by_its_kernel_name() {
    let sys = tempfile::tempdir().unwrap();
    fs::create_dir(sys.path().join("7:3")).unwrap();
    fs::write(
        sys.path().join("7:3/uevent"),
        "MAJOR=7\nMINOR=3\nDEVNAME=loop3\n",
    )
    .unwrap();
    fs::create_dir(sys.path().join("7:4")).unwrap();
    fs::write(sys.path().join("7:4/uevent"), "MAJOR=7\nMINOR=4\n").unwrap();

    assert_eq!(
        super::loop_node(sys.path(), 7, 3).unwrap(),
        Path::new("/dev/loop3")
    );
    super::loop_node(sys.path(), 7, 4).unwrap_err();
    super::loop_node(sys.path(), 7, 5).unwrap_err();
    let disk = super::loop_node(sys.path(), 8, 0).unwrap_err();
    assert!(
        disk.to_string().contains("input/output"),
        "a disk is never hashed: {disk}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_device_is_verified_by_its_bytes_and_a_wrong_one_is_detached() {
    let dir = tempfile::tempdir().unwrap();
    let device = dir.path().join("loop3");
    fs::write(&device, BYTES).unwrap();
    let digest = sha256_of(&device).unwrap();
    let capture = afd_core::test_util::trace::Capture::install();

    Toolbox::at(dir.path().to_owned(), digest)
        .verify_device(&device)
        .unwrap();
    let wrong = Toolbox::at(dir.path().to_owned(), "0".repeat(64));
    let refused = wrong.verify_device(&device).unwrap_err();
    wrong.detach();

    assert!(refused.to_string().contains("does not hash"), "{refused}");
    // Nothing is mounted there, so the detach is refused and said so.
    assert!(
        capture
            .only("sandbox_toolbox_mount_failed")
            .field("error_code")
            .is_some()
    );
}
