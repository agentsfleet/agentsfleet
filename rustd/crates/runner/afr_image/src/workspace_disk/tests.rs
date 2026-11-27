//! A workspace image's caching names, and the image `format_image` makes.

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use super::{Caching, format_image};
use crate::host::HostTools;

/// A formatter that formats nothing, successfully.
const SUCCEEDS: &str = "/usr/bin/true";
/// The image's size.
const DISK_BYTES: u64 = 4_096;

#[test]
fn test_each_caching_names_itself() {
    assert_eq!(Caching::Direct.as_str(), "direct");
    assert_eq!(Caching::Buffered.as_str(), "buffered");
}

/// The image is sparse at its size and readable by its owner alone, and an
/// image already there is never overwritten.
#[tokio::test]
#[expect(clippy::unwrap_used, reason = "test setup and assertions")]
async fn test_an_image_is_made_at_its_size_for_its_owner_alone() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("workspace.img");
    let tools = HostTools {
        mke2fs: PathBuf::from(SUCCEEDS),
    };

    format_image(&tools, &image, DISK_BYTES, (1000, 100))
        .await
        .unwrap();

    let made = std::fs::metadata(&image).unwrap();
    assert_eq!(made.len(), DISK_BYTES);
    assert_eq!(made.permissions().mode() & 0o777, 0o600);
    assert!(
        format_image(&tools, &image, DISK_BYTES, (1000, 100))
            .await
            .is_err()
    );
}
