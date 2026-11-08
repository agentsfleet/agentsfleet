//! Staging: an image enters the host's toolbox directory only through a
//! private staging directory, copied and hashed in one pass, synced, and
//! published with one rename. A reader never sees a half-written image under a
//! digest's name, and a runner killed mid-copy leaves only a partial file the
//! next start sweeps away. Each copy has a file of its own, so two admissions
//! of one release never write into the same one; the directory is one
//! runner's, so the start's sweep takes no other runner's copy.

use std::fs::{self, DirBuilder, File, Permissions};
use std::io::{self, BufReader, Read as _};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use digest_io::HashWriter;
use sha2::{Digest as _, Sha256};
use tempfile::Builder;

use super::Manifest;
use crate::error::Result;

/// The staging directory, under the host's toolbox directory.
const STAGE_DIR: &str = ".stage";
/// The staging directory is the runner's alone.
const STAGE_MODE: u32 = 0o700;
/// A published image: anyone may read it, nobody may write it.
const IMAGE_MODE: u32 = 0o444;
/// What a copy in progress is named, after its digest and a part of its own.
const PARTIAL_SUFFIX: &str = ".partial";
/// How much of an image one read takes: images run to gigabytes.
const COPY_BUFFER_BYTES: usize = 1024 * 1024;
/// The event a partial file a previous run left is logged under.
const EVENT_PARTIAL_SWEPT: &str = "sandbox_toolbox_partial_swept";

/// Where the image `manifest` names is published under `dir`.
#[must_use]
pub(super) fn published(dir: &Path, manifest: &Manifest) -> PathBuf {
    dir.join(super::image_name(manifest.sha256()))
}

/// Copies `source` into the staging directory under `dir`, hashing as it
/// goes, and publishes it as [`published`] once its length and digest are the
/// manifest's and its bytes are on disk.
///
/// # Errors
/// The source cannot be read or the copy written, or the bytes are not the
/// manifest's image; nothing is published then.
pub(super) fn stage(manifest: &Manifest, source: &Path, dir: &Path) -> Result<PathBuf> {
    let staging = dir.join(STAGE_DIR);
    DirBuilder::new()
        .recursive(true)
        .mode(STAGE_MODE)
        .create(&staging)?;
    // The digest leads the name and the suffix ends it, so the sweep knows a
    // copy; the part between is the copy's own, so no two ever share a file.
    let digest = format!("{}.", manifest.sha256());
    let mut partial = Builder::new()
        .prefix(&digest)
        .suffix(PARTIAL_SUFFIX)
        .permissions(Permissions::from_mode(IMAGE_MODE))
        .tempfile_in(&staging)?;
    // A refusal drops the copy, which removes it; one a crash leaves is the
    // next start's sweep.
    copy_verified(manifest, source, partial.as_file_mut())?;
    let image = published(dir, manifest);
    // A rename the kernel refuses hands the copy back, and dropping it
    // removes it; the rename's own failure is what the caller reads.
    partial.persist(&image).map_err(io::Error::from)?;
    File::open(dir)?.sync_all()?;
    Ok(image)
}

/// Copies `source` into `output`, at most one byte past the manifest's
/// length, and syncs it once its length and digest are the manifest's.
fn copy_verified(manifest: &Manifest, source: &Path, output: &mut File) -> Result<()> {
    let mut input = BufReader::with_capacity(
        COPY_BUFFER_BYTES,
        File::open(source)?.take(manifest.length().saturating_add(1)),
    );
    let mut writer = HashWriter::<Sha256, _>::new(output);
    let copied = io::copy(&mut input, &mut writer)?;
    let (sha256, output) = writer.into_parts();
    manifest.check_length(copied)?;
    manifest.check_digest(&hex::encode(sha256.finalize()))?;
    Ok(output.sync_all()?)
}

/// Removes every partial copy a previous run left in staging; how many.
/// Anything else there, which no runner put there, is left for whoever did.
///
/// # Errors
/// The staging directory cannot be read, or a partial file removed.
pub(super) fn sweep(dir: &Path) -> Result<usize> {
    let entries = match fs::read_dir(dir.join(STAGE_DIR)) {
        Ok(entries) => entries,
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(unreadable) => return Err(unreadable.into()),
    };
    let mut swept = 0;
    for entry in entries {
        let entry = entry?;
        let partial_copy = entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX);
        if !partial_copy {
            continue;
        }
        let path = entry.path();
        fs::remove_file(&path)?;
        let partial = path.display().to_string();
        let event = EVENT_PARTIAL_SWEPT;
        tracing::info!(partial, event);
        swept += 1;
    }
    Ok(swept)
}

#[cfg(test)]
mod tests;
