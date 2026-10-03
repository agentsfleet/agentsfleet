//! Asking the kernel about and detaching the mounts this crate makes.

use std::path::Path;

/// Whether `path` is the root of a mount, as the kernel reports through
/// `statx`; a path that cannot be read, or a kernel too old to say, is not.
#[cfg(target_os = "linux")]
pub(crate) fn is_mount_root(path: &Path) -> bool {
    use rustix::fs::{AtFlags, CWD, StatxAttributes, StatxFlags};
    rustix::fs::statx(
        CWD,
        path,
        AtFlags::NO_AUTOMOUNT | AtFlags::SYMLINK_NOFOLLOW,
        StatxFlags::empty(),
    )
    .is_ok_and(|found| {
        found
            .stx_attributes_mask
            .contains(StatxAttributes::MOUNT_ROOT)
            && found.stx_attributes.contains(StatxAttributes::MOUNT_ROOT)
    })
}

/// No mount this crate makes exists off Linux.
#[cfg(not(target_os = "linux"))]
pub(crate) fn is_mount_root(_path: &Path) -> bool {
    false
}

/// Unmounts `path`. `detach` lets processes still using it keep it until
/// they let go, which is how a replaced toolbox stays under running sandboxes.
#[cfg(target_os = "linux")]
pub(crate) fn unmount(path: &Path, detach: bool) -> crate::Result<()> {
    let flags = if detach {
        rustix::mount::UnmountFlags::DETACH
    } else {
        rustix::mount::UnmountFlags::empty()
    };
    Ok(rustix::mount::unmount(path, flags)?)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::path::Path;

    #[test]
    fn test_a_mount_root_is_told_from_a_plain_directory() {
        let dir = tempfile::tempdir().ok();

        assert!(super::is_mount_root(Path::new("/proc")));
        assert!(!dir.is_some_and(|dir| super::is_mount_root(dir.path())));
    }

    #[test]
    fn test_unmounting_what_is_not_mounted_is_refused() {
        let dir = tempfile::tempdir().ok();
        let path = dir
            .as_ref()
            .map_or(Path::new("/nonexistent"), |dir| dir.path());

        assert!(super::unmount(path, true).is_err());
        assert!(super::unmount(path, false).is_err());
    }
}
