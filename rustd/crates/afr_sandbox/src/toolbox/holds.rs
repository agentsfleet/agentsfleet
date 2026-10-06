//! Which toolbox images a host keeps: the current and previous releases, and
//! any image a lease or warm slot still holds.
//!
//! A sandbox holds the toolbox it runs on by keeping a clone of its `Arc`, so
//! a hold is a strong count, and an image whose only `Arc` is this registry's
//! is held by nothing. Retention runs at every admission and whenever the host
//! calls [`Toolboxes::retain`]; an image held past its release goes at the
//! first of those after its last sandbox does.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::{Manifest, Toolbox, stage};
use crate::error::{Result, ToolboxRefusal};

/// How many of the newest releases a host keeps mounted whether or not a
/// sandbox holds them: the current one, and the previous one to roll back to.
pub const TOOLBOX_KEEP_RELEASES: usize = 2;
/// The event an image retention let go of is logged under.
const EVENT_RELEASED: &str = "sandbox_toolbox_released";
/// The event an image retention could not let go of is logged under.
const EVENT_RELEASE_FAILED: &str = "sandbox_toolbox_release_failed";
/// The event a published image staged again is logged under.
const EVENT_RESTAGED: &str = "sandbox_toolbox_restaged";
/// The refusals that say the published file is not the manifest's image, so
/// a fresh copy of the download may be.
const REFUSALS_OF_THE_FILE: [ToolboxRefusal; 3] = [
    ToolboxRefusal::Length,
    ToolboxRefusal::Digest,
    ToolboxRefusal::NotAFile,
];

/// Mounts an admitted image and unmounts one: the kernel on a host, a
/// recorder in the unit suites.
pub trait Mounter: fmt::Debug + Send + Sync {
    /// Mounts `image`, the published file `manifest` names.
    ///
    /// # Errors
    /// The image is not the manifest's, or the mount is refused.
    fn mount(&self, manifest: &Manifest, image: &Path) -> Result<Toolbox>;

    /// Unmounts `toolbox`, which no sandbox holds.
    ///
    /// # Errors
    /// The kernel refuses the unmount.
    fn unmount(&self, toolbox: &Toolbox) -> Result<()>;
}

/// The images one host has admitted, oldest first, under one directory.
#[derive(Debug)]
pub struct Toolboxes<M> {
    dir: PathBuf,
    mounter: M,
    admitted: Mutex<Vec<Arc<Toolbox>>>,
}

impl<M: Mounter> Toolboxes<M> {
    /// The images under `dir`, after sweeping what an interrupted runner left
    /// in staging.
    ///
    /// # Errors
    /// The staging directory cannot be read or a partial file removed.
    pub fn open(dir: PathBuf, mounter: M) -> Result<Self> {
        stage::sweep(&dir)?;
        Ok(Self {
            dir,
            mounter,
            admitted: Mutex::new(Vec::new()),
        })
    }

    /// Admits the image `manifest` names as the current release, staging it
    /// from `source` first when it is not yet published, then lets go of what
    /// retention no longer keeps. An old image retention cannot let go of is
    /// logged and stays admitted for the next pass: it does not refuse the new
    /// release. Blocks: it reads the whole image.
    ///
    /// # Errors
    /// The image cannot be staged or admitted; the releases already admitted
    /// stay as they were.
    pub fn admit(&self, manifest: &Manifest, source: &Path) -> Result<Arc<Toolbox>> {
        let image = stage::published(&self.dir, manifest);
        let fresh = !image.exists();
        if fresh {
            stage::stage(manifest, source, &self.dir)?;
        }
        let mut admitted = self.admitted();
        let existing = admitted
            .iter()
            .position(|toolbox| toolbox.digest() == manifest.sha256());
        let toolbox = match existing {
            Some(index) => admitted.remove(index),
            None => Arc::new(self.mount(manifest, source, &image, fresh)?),
        };
        admitted.push(Arc::clone(&toolbox));
        if let Err(stuck) = self.retain_in(&mut admitted) {
            let error_code = stuck.code().as_str();
            let reason = stuck.to_string();
            let event = EVENT_RELEASE_FAILED;
            tracing::warn!(error_code, reason, event);
        }
        Ok(toolbox)
    }

    /// Unmounts and removes every image past the newest
    /// [`TOOLBOX_KEEP_RELEASES`] that no sandbox holds; how many went.
    ///
    /// # Errors
    /// An unmount was refused; that image stays admitted.
    pub fn retain(&self) -> Result<usize> {
        self.retain_in(&mut self.admitted())
    }

    /// The digests admitted, oldest first.
    #[must_use]
    pub fn digests(&self) -> Vec<String> {
        self.admitted()
            .iter()
            .map(|toolbox| toolbox.digest().to_owned())
            .collect()
    }

    /// Unmounts every admitted image and removes its published file; the
    /// host is shutting down, and its next start stages each release it
    /// admits again, so no image outlives the list that would let it go.
    ///
    /// # Errors
    /// An unmount was refused; the images not yet unmounted stay admitted.
    pub fn close(&self) -> Result<()> {
        let mut admitted = self.admitted();
        while let Some(toolbox) = admitted.pop() {
            if let Err(refused) = self.release(&toolbox) {
                admitted.push(toolbox);
                return Err(refused);
            }
        }
        Ok(())
    }

    /// Mounts the published `image`. One published before this admission
    /// that is no longer the manifest's image, rotted, edited in place or
    /// swapped for a link, is removed and staged again from `source`, once.
    fn mount(
        &self,
        manifest: &Manifest,
        source: &Path,
        image: &Path,
        fresh: bool,
    ) -> Result<Toolbox> {
        match self.mounter.mount(manifest, image) {
            Err(refused)
                if !fresh
                    && refused
                        .toolbox_refusal()
                        .is_some_and(|refusal| REFUSALS_OF_THE_FILE.contains(&refusal)) =>
            {
                let digest = manifest.sha256();
                let reason = refused.to_string();
                let event = EVENT_RESTAGED;
                tracing::warn!(digest, reason, event);
                fs::remove_file(image)?;
                stage::stage(manifest, source, &self.dir)?;
                self.mounter.mount(manifest, image)
            }
            mounted => mounted,
        }
    }

    fn retain_in(&self, admitted: &mut Vec<Arc<Toolbox>>) -> Result<usize> {
        let mut released = 0;
        let mut index = 0;
        while index < admitted.len().saturating_sub(TOOLBOX_KEEP_RELEASES) {
            let held = admitted
                .get(index)
                .is_some_and(|toolbox| Arc::strong_count(toolbox) > 1);
            if held {
                index += 1;
                continue;
            }
            let toolbox = admitted.remove(index);
            if let Err(refused) = self.release(&toolbox) {
                admitted.insert(index, toolbox);
                return Err(refused);
            }
            released += 1;
        }
        Ok(released)
    }

    /// Removes `toolbox`'s published image, then unmounts it, so an image
    /// still on the list is always still mounted: one whose file will not go
    /// stays mounted, and one whose unmount is refused stays mounted with its
    /// file already gone, which the next pass takes in its stride.
    fn release(&self, toolbox: &Toolbox) -> Result<()> {
        let image = self.dir.join(super::image_name(toolbox.digest()));
        match fs::remove_file(&image) {
            Err(failed) if failed.kind() != io::ErrorKind::NotFound => return Err(failed.into()),
            _removed_or_already_gone => {}
        }
        self.mounter.unmount(toolbox)?;
        let digest = toolbox.digest();
        let event = EVENT_RELEASED;
        tracing::info!(digest, event);
        Ok(())
    }

    /// The admitted images, whoever last held the lock: the list stays whole
    /// even if a holder panicked, because every change to it is one push,
    /// remove or insert.
    fn admitted(&self) -> MutexGuard<'_, Vec<Arc<Toolbox>>> {
        self.admitted.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "holds/tests.rs"]
mod tests;
