//! Staging: an image enters the host's toolbox directory only through a
//! private staging directory, copied and hashed in one pass, synced, and
//! published with one rename. A reader never sees a half-written image under a
//! digest's name, and a runner killed mid-copy leaves only a partial file the
//! next start sweeps away.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::Manifest;
use crate::error::Result;

/// The staging directory, under the host's toolbox directory.
const STAGE_DIR: &str = ".stage";
/// The staging directory is the runner's alone.
const STAGE_MODE: u32 = 0o700;
/// A published image: anyone may read it, nobody may write it.
const IMAGE_MODE: u32 = 0o444;
/// What a copy in progress is named after its digest.
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
    let partial = staging.join(format!("{}{PARTIAL_SUFFIX}", manifest.sha256()));
    if let Err(refused) = copy_verified(manifest, source, &partial) {
        // The refusal is what the caller needs; a partial the removal leaves
        // is the next start's sweep.
        let _left = fs::remove_file(&partial);
        return Err(refused);
    }
    let image = published(dir, manifest);
    fs::rename(&partial, &image)?;
    File::open(dir)?.sync_all()?;
    Ok(image)
}

/// Copies `source` to `partial`, at most one byte past the manifest's length,
/// and syncs it once its length and digest are the manifest's.
fn copy_verified(manifest: &Manifest, source: &Path, partial: &Path) -> Result<()> {
    let mut input = File::open(source)?.take(manifest.length().saturating_add(1));
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(IMAGE_MODE)
        .open(partial)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; COPY_BUFFER_BYTES];
    let mut copied: u64 = 0;
    loop {
        let read = input.read(&mut buffer)?;
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        hasher.update(chunk);
        output.write_all(chunk)?;
        copied = copied.saturating_add(chunk.len() as u64);
    }
    manifest.check_length(copied)?;
    manifest.check_digest(&hex::encode(hasher.finalize()))?;
    Ok(output.sync_all()?)
}

/// Removes every partial copy a previous run left in staging; how many.
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
        let path = entry?.path();
        fs::remove_file(&path)?;
        let partial = path.display().to_string();
        let event = EVENT_PARTIAL_SWEPT;
        tracing::info!(partial, event);
        swept += 1;
    }
    Ok(swept)
}

#[cfg(test)]
#[path = "stage/tests.rs"]
mod tests;
