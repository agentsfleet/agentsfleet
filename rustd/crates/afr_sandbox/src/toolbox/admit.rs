//! Admission by descriptor: the published image opened once, checked, hashed
//! and attached through that one descriptor, then mounted.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use digest_io::IoWrapper;
use rustix::fs::FileType;
use rustix::mount::MountFlags;
use sha2::{Digest as _, Sha256};

use super::adopt::{self, EROFS};
use super::{Manifest, Toolbox, loop_device};
use crate::error::{Result, ToolboxRefusal, toolbox_refused};

/// How much of an image one read takes while it is hashed: images run to
/// gigabytes, and the hash reads through this buffer rather than its own.
const HASH_BUFFER_BYTES: usize = 1024 * 1024;
/// The event an admission's start is logged under.
const EVENT_ADMISSION_STARTED: &str = "sandbox_toolbox_admission_started";
/// The event a mounted, admitted toolbox is logged under.
const EVENT_ADMISSION_COMPLETED: &str = "sandbox_toolbox_admission_completed";
/// The event a toolbox that was not admitted is logged under.
const EVENT_ADMISSION_FAILED: &str = "sandbox_toolbox_admission_failed";

impl Toolbox {
    /// Admits the published image at `image`, which `manifest` names, and
    /// mounts it at `<mounts>/<digest>`: opened once without following a
    /// link, checked to be a regular file of the manifest's length, hashed
    /// through that descriptor, attached read-only to a loop device from the
    /// same descriptor, and mounted `ro,nosuid,nodev`. A mount already there is
    /// adopted only when it is this file, mounted that way.
    ///
    /// # Errors
    /// A refusal naming the check the image failed, or a mount the kernel
    /// would not make; nothing stays mounted then.
    pub async fn admit(manifest: &Manifest, image: &Path, mounts: &Path) -> Result<Self> {
        let digest = manifest.sha256().to_owned();
        let event = EVENT_ADMISSION_STARTED;
        tracing::info!(digest, event);
        let (manifest, image, mounts) = (manifest.clone(), image.to_owned(), mounts.to_owned());
        let admitted =
            tokio::task::spawn_blocking(move || Self::admit_now(&manifest, &image, &mounts))
                .await?;
        match &admitted {
            Ok(_) => {
                let event = EVENT_ADMISSION_COMPLETED;
                tracing::info!(digest, event);
            }
            Err(error) => {
                let error_code = error.code().as_str();
                let refusal = error.toolbox_refusal().map(ToolboxRefusal::as_str);
                let reason = error.to_string();
                let event = EVENT_ADMISSION_FAILED;
                tracing::error!(digest, error_code, refusal, reason, event);
            }
        }
        admitted
    }

    /// [`Self::admit`], on the calling thread: it reads the whole image.
    pub(crate) fn admit_now(manifest: &Manifest, image: &Path, mounts: &Path) -> Result<Self> {
        let file = open_image(image)?;
        let stat = rustix::fs::fstat(&file)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(toolbox_refused(
                ToolboxRefusal::NotAFile,
                "not a regular file",
            ));
        }
        manifest.check_length(u64::try_from(stat.st_size).unwrap_or(0))?;
        let digest = sha256_of(&file)?;
        manifest.check_digest(&digest)?;
        let toolbox = Self {
            root: mounts.join(&digest),
            digest,
        };
        if adopt::adopt(&toolbox.root, (stat.st_dev, stat.st_ino))? {
            return Ok(toolbox);
        }
        fs::create_dir_all(&toolbox.root)?;
        let attached = loop_device::attach(&file)?;
        let flags = MountFlags::RDONLY | MountFlags::NOSUID | MountFlags::NODEV;
        rustix::mount::mount(&attached.node, &toolbox.root, EROFS, flags, None)?;
        Ok(toolbox)
    }

    /// Detaches the host's mount of the image and removes its mount point;
    /// the loop device detaches itself as the mount lets go. Sandboxes
    /// already started keep their own bind of it.
    ///
    /// # Errors
    /// The kernel refuses the unmount, or the mount point cannot be removed.
    pub fn unmount(&self) -> Result<()> {
        crate::mounts::unmount(&self.root, false)?;
        Ok(fs::remove_dir(&self.root)?)
    }
}

/// `image`, opened for reading without following a link and without waiting
/// on a pipe someone put there.
fn open_image(image: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(image)
        .map_err(|refused| {
            if refused.raw_os_error() == Some(libc::ELOOP) {
                toolbox_refused(ToolboxRefusal::NotAFile, "a symbolic link")
            } else {
                refused.into()
            }
        })
}

/// The SHA-256 of everything `file` holds, read through its descriptor.
fn sha256_of(file: &File) -> Result<String> {
    let mut hasher = IoWrapper(Sha256::new());
    io::copy(
        &mut BufReader::with_capacity(HASH_BUFFER_BYTES, file),
        &mut hasher,
    )?;
    Ok(hex::encode(hasher.0.finalize()))
}
