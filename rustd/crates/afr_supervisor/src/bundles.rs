//! Fleet bundles, fetched by content hash and verified before they are kept.
//!
//! A bundle's name is the digest the daemon computed at import over its
//! documents and support files — not over the tar that carries them — so
//! verifying means reading the tar back into those parts and recomputing the
//! same digest: `SKILL.md`, a zero byte, `TRIGGER.md` (empty when absent), a
//! zero byte, then each support file's path, a zero byte, its content and a
//! zero byte. Bytes that do not hash to their name are refused and never
//! cached, so a tampered or truncated download cannot be run, now or later.

use std::io::{self, Read, Write as _};
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;

use afd_core::bundle::{self, BundleDigest};
use tempfile::NamedTempFile;

use crate::client::{ControlPlane, retrying};
use crate::error::{self, Result};
use crate::storage_home::StorageHome;

/// The bundle's root document.
const SKILL_PATH: &str = "SKILL.md";
/// The bundle's optional trigger document.
const TRIGGER_PATH: &str = "TRIGGER.md";
/// The extension a cached bundle carries.
const EXTENSION: &str = "tar";
const EVENT_CACHE_HIT: &str = "bundle_cache_hit";
const EVENT_MATERIALIZED: &str = "bundle_materialized";
const EVENT_SKILL_ONLY: &str = "bundle_skill_only";

/// The verified-bundle cache under the storage home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleCache {
    dir: Arc<Path>,
}

impl BundleCache {
    /// The cache under `home`.
    pub(crate) fn new(home: &StorageHome) -> Self {
        Self {
            dir: home.bundles().into(),
        }
    }

    /// The verified bundle named `content_hash`, from the cache or the daemon,
    /// or `None` for a skill-only bundle.
    ///
    /// A cached copy is verified again before it is trusted; one that no longer
    /// verifies is fetched afresh. A bundle with no support files stores no
    /// snapshot, so the daemon answers 404 for it and the run goes on with none
    /// (`afd_wire::lease::BundleManifest`).
    ///
    /// # Errors
    /// A name that is not a digest, a download that fails or does not verify,
    /// or a cache write that fails.
    pub(crate) async fn fetch(
        &self,
        plane: &ControlPlane,
        content_hash: &str,
    ) -> Result<Option<Bundle>> {
        if !bundle::is_name(content_hash) {
            return Err(error::tampered(content_hash));
        }
        let path = self.dir.join(content_hash).with_extension(EXTENSION);
        if let Some(cached) = tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|bytes| verified(&bytes, content_hash))
        {
            let event = EVENT_CACHE_HIT;
            tracing::debug!(content_hash, event);
            return Ok(Some(cached));
        }
        let bytes = match retrying(|| plane.bundle(content_hash)).await {
            Ok(bytes) => bytes,
            Err(absent) if absent.is_not_found() => {
                let event = EVENT_SKILL_ONLY;
                tracing::debug!(content_hash, event);
                return Ok(None);
            }
            Err(failure) => return Err(failure),
        };
        let bundle = verified(&bytes, content_hash).ok_or_else(|| error::tampered(content_hash))?;
        let dir = Arc::clone(&self.dir);
        tokio::task::spawn_blocking(move || keep(&dir, &path, &bytes)).await??;
        let event = EVENT_MATERIALIZED;
        tracing::info!(content_hash, event);
        Ok(Some(bundle))
    }
}

/// Writes a verified bundle into the cache through a temporary file, so a
/// crash never leaves half an archive under a bundle's name.
fn keep(dir: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.persist(path)?;
    Ok(())
}

/// A verified Fleet Bundle: its documents and the support files a lease's
/// workspace receives, read once from the canonical archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bundle {
    skill: Bytes,
    trigger: Bytes,
    files: Vec<(String, Bytes)>,
}

impl Bundle {
    /// Reads a canonical bundle archive, or `None` when the bytes are not one:
    /// unreadable, or not opening with `SKILL.md`.
    fn read(tar: &[u8]) -> Option<Self> {
        let mut parts = tar::Archive::new(tar)
            .entries()
            .ok()?
            .map(|entry| read_entry(entry, tar.len()))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .peekable();
        let (skill_path, skill) = parts.next()?;
        if skill_path != SKILL_PATH {
            return None;
        }
        let trigger = parts
            .next_if(|(path, _)| path == TRIGGER_PATH)
            .map_or_else(Bytes::new, |(_, trigger)| trigger);
        Some(Self {
            skill,
            trigger,
            files: parts.collect(),
        })
    }

    /// Whether this is the bundle the importer named `content_hash`.
    fn is_named(&self, content_hash: &str) -> bool {
        let mut digest = BundleDigest::new(&self.skill, Some(&self.trigger));
        for (path, content) in &self.files {
            digest.support_file(path, content);
        }
        digest.matches(content_hash)
    }

    /// The support files, by their path inside the workspace.
    pub(crate) fn support_files(&self) -> &[(String, Bytes)] {
        &self.files
    }
}

/// One entry's path and content, or `None` when it does not read.
///
/// The content is read into a buffer sized by the entry's header, but never
/// past `archive_len`: the archive is read before it is verified, and a header
/// may claim more than the archive holds.
fn read_entry<R: Read>(
    entry: io::Result<tar::Entry<'_, R>>,
    archive_len: usize,
) -> Option<(String, Bytes)> {
    let mut entry = entry.ok()?;
    let path = entry.path().ok()?.to_str()?.to_owned();
    let claimed = usize::try_from(entry.size()).unwrap_or(archive_len);
    let mut content = Vec::with_capacity(claimed.min(archive_len));
    entry.read_to_end(&mut content).ok()?;
    Some((path, Bytes::from(content)))
}

/// The bundle `bytes` hold, when they read and carry `content_hash`'s name.
fn verified(bytes: &[u8], content_hash: &str) -> Option<Bundle> {
    Bundle::read(bytes).filter(|bundle| bundle.is_named(content_hash))
}

#[cfg(test)]
#[path = "bundles/tests.rs"]
mod tests;
