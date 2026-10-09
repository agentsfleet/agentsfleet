//! The release a deploy staged for this host: one image, its release manifest
//! and the manifest's signature side by side in one directory, as
//! `deploy/baremetal/toolbox.sh` lays them out and `scripts/toolbox/build.sh`
//! and cosign name them. The runner admits that release at boot.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::manifest::MANIFEST_MAX_BYTES;
use super::{Mounter, Release, TOOLBOX_PREFIX, Toolbox, Toolboxes, image_name};
use crate::error::{Result, ToolboxRefusal, toolbox_refused};

/// A staged release manifest's name: [`TOOLBOX_PREFIX`], the image's digest,
/// then this.
pub const MANIFEST_SUFFIX: &str = ".json";
/// Its signature's name: the manifest's, then this.
pub const SIGNATURE_SUFFIX: &str = ".sig";

impl<M: Mounter> Toolboxes<M> {
    /// Admits the one release staged under `incoming`, its manifest checked by
    /// `release` before the image is read. Blocks: it reads the whole image.
    ///
    /// # Errors
    /// `incoming` holds no staged release or more than one, its manifest or
    /// signature cannot be read, or admission refuses the release.
    pub fn admit_incoming(&self, release: &Release, incoming: &Path) -> Result<Arc<Toolbox>> {
        let manifest = staged_manifest(incoming)?;
        let verified = release.verify(&bounded(&manifest)?, &bounded(&signature_of(&manifest))?)?;
        self.admit(&verified, &incoming.join(image_name(verified.sha256())))
    }
}

/// The path of the one release manifest under `incoming`.
fn staged_manifest(incoming: &Path) -> Result<PathBuf> {
    let staged = fs::read_dir(incoming)?
        .map(|entry| entry.map(|entry| entry.path()))
        .filter(|path| path.as_ref().map_or(true, |path| is_manifest(path)))
        .collect::<std::io::Result<Vec<_>>>()?;
    match <[PathBuf; 1]>::try_from(staged) {
        Ok([manifest]) => Ok(manifest),
        Err(staged) => {
            let detail = format!(
                "{} releases are staged in {}; a deploy stages exactly one",
                staged.len(),
                incoming.display()
            );
            Err(toolbox_refused(ToolboxRefusal::Unstaged, detail))
        }
    }
}

/// Whether `path` names a release manifest.
fn is_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(TOOLBOX_PREFIX) && name.ends_with(MANIFEST_SUFFIX))
}

/// Where the signature over `manifest` is staged.
fn signature_of(manifest: &Path) -> PathBuf {
    let mut name = manifest.as_os_str().to_owned();
    name.push(SIGNATURE_SUFFIX);
    PathBuf::from(name)
}

/// `path`'s bytes, read one byte past the most a manifest or signature may
/// hold, so an oversized file is refused by [`Release::verify`] rather than
/// read whole.
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let limit = u64::try_from(MANIFEST_MAX_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "incoming/tests.rs"]
mod tests;
