//! One microVM per workspace image: an exclusive lock taken on the image
//! before its drive is attached and held while its monitor runs, so a second
//! attach is refused before a second monitor starts.
//!
//! Two monitors writing one ext4 image corrupt it — the guest reads
//! `checksum invalid` — and Firecracker takes no lock of its own. The lock is
//! the kernel's `flock` on the image's inode, so the hard link in a jail is
//! the same lock, and a runner that dies drops it with its descriptor.
//!
//! One engine per state directory, the same way: [`StateLock`] is taken
//! before the engine's boot sweep, which removes every sandbox directory it
//! finds, so a second runner started over a serving one's directory is
//! refused instead of removing its live microVMs.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use afr_engine::Result;

use crate::error::image_held;
#[cfg(target_os = "linux")]
use crate::error::state_held;

/// The lock file's name in a state directory: no sandbox identifier, so no
/// sweep reads it as a sandbox.
#[cfg(target_os = "linux")]
const STATE_LOCK_FILE: &str = "engine.lock";

/// The exclusive lock on one state directory, released when dropped.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct StateLock {
    /// Kept open: the lock lives as long as this descriptor.
    _file: File,
}

#[cfg(target_os = "linux")]
impl StateLock {
    /// Takes the lock on `state_dir`, made when absent, without waiting.
    ///
    /// # Errors
    /// Another engine holds it, which is refused naming the lock file, or
    /// the directory or the file cannot be made.
    pub(crate) fn take(state_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let lock = state_dir.join(STATE_LOCK_FILE);
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(state_held(lock)),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }
}

/// The exclusive lock on one workspace image, released when dropped.
#[derive(Debug)]
pub struct ImageLock {
    /// Kept open: the lock lives as long as this descriptor.
    _file: File,
    image: PathBuf,
}

impl ImageLock {
    /// Takes the lock on `image` without waiting.
    ///
    /// # Errors
    /// Another holder has it, which is refused naming the image, or the image
    /// cannot be opened.
    pub fn take(image: &Path) -> Result<Self> {
        let file = File::options().read(true).write(true).open(image)?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                _file: file,
                image: image.to_owned(),
            }),
            Err(TryLockError::WouldBlock) => Err(image_held(image.to_owned())),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    /// The image it holds.
    #[must_use]
    pub fn image(&self) -> &Path {
        &self.image
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use std::fs;

    use super::ImageLock;

    #[test]
    fn test_a_second_attach_of_a_held_workspace_image_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("workspace.img");
        fs::write(&image, b"").unwrap();
        let linked = dir.path().join("linked.img");
        fs::hard_link(&image, &linked).unwrap();

        let held = ImageLock::take(&image).unwrap();
        let refused = ImageLock::take(&linked).unwrap_err();

        let reason = refused.detail();
        assert!(
            reason.contains("is locked by a running microVM"),
            "{reason}"
        );
        assert!(reason.contains("linked.img"), "{reason}");
        assert_eq!(held.image(), image);
        drop(held);
        let retaken = ImageLock::take(&linked).unwrap();
        assert_eq!(retaken.image(), linked);
    }
}
