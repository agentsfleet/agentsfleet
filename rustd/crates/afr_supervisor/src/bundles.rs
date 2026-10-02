//! Fleet bundles, fetched by content hash and verified before they are kept.
//!
//! A bundle's name is the digest the daemon computed at import over its
//! documents and support files — not over the tar that carries them — so
//! verifying means reading the tar back into those parts and recomputing the
//! same digest: `SKILL.md`, a zero byte, `TRIGGER.md` (empty when absent), a
//! zero byte, then each support file's path, a zero byte, its content and a
//! zero byte. Bytes that do not hash to their name are refused and never
//! cached, so a tampered or truncated download cannot be run, now or later.

use std::fs;
use std::io::{self, Read, Write as _};
use std::path::PathBuf;

use afd_core::bundle::BundleDigest;
use tempfile::NamedTempFile;

use crate::client::{ControlPlane, retrying};
use crate::error::{self, Result};
use crate::storage_home::StorageHome;

/// The bundle's root document.
const SKILL_PATH: &str = "SKILL.md";
/// The bundle's optional trigger document.
const TRIGGER_PATH: &str = "TRIGGER.md";
/// A SHA-256 digest's length in hexadecimal.
const DIGEST_HEX_LEN: usize = 64;
/// The extension a cached bundle carries.
const EXTENSION: &str = "tar";
const EVENT_CACHE_HIT: &str = "bundle_cache_hit";
const EVENT_MATERIALIZED: &str = "bundle_materialized";
const EVENT_SKILL_ONLY: &str = "bundle_skill_only";

/// The verified-bundle cache under the storage home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleCache {
    dir: PathBuf,
}

impl BundleCache {
    /// The cache under `home`.
    pub(crate) fn new(home: &StorageHome) -> Self {
        Self {
            dir: home.bundles(),
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
    ) -> Result<Option<PathBuf>> {
        if !is_digest(content_hash) {
            return Err(error::tampered(content_hash));
        }
        let path = self.dir.join(content_hash).with_extension(EXTENSION);
        if fs::read(&path).is_ok_and(|cached| digest(&cached).as_deref() == Some(content_hash)) {
            let event = EVENT_CACHE_HIT;
            tracing::debug!(content_hash, event);
            return Ok(Some(path));
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
        if digest(&bytes).as_deref() != Some(content_hash) {
            return Err(error::tampered(content_hash));
        }
        let mut file = NamedTempFile::new_in(&self.dir)?;
        file.write_all(&bytes)?;
        file.persist(&path)?;
        let event = EVENT_MATERIALIZED;
        tracing::info!(content_hash, event);
        Ok(Some(path))
    }
}

/// Whether `name` is a lowercase SHA-256 hex digest.
fn is_digest(name: &str) -> bool {
    name.len() == DIGEST_HEX_LEN
        && name
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The import digest of a canonical bundle tar, or `None` when the bytes are
/// not one: unreadable, or not opening with `SKILL.md`.
fn digest(tar: &[u8]) -> Option<String> {
    let parts = tar::Archive::new(tar)
        .entries()
        .ok()?
        .map(read_entry)
        .collect::<Option<Vec<_>>>()?;
    let ((skill_path, skill), rest) = parts.split_first()?;
    if skill_path != SKILL_PATH {
        return None;
    }
    let (trigger, files) = match rest.split_first() {
        Some(((path, trigger), files)) if path == TRIGGER_PATH => (trigger.as_slice(), files),
        _absent => (&[][..], rest),
    };
    let mut digest = BundleDigest::new(skill, Some(trigger));
    for (path, content) in files {
        digest.support_file(path, content);
    }
    Some(digest.finish())
}

/// One entry's path and content, or `None` when it does not read.
fn read_entry<R: Read>(entry: io::Result<tar::Entry<'_, R>>) -> Option<(String, Vec<u8>)> {
    let mut entry = entry.ok()?;
    let path = entry.path().ok()?.to_str()?.to_owned();
    let mut content = Vec::new();
    entry.read_to_end(&mut content).ok()?;
    Some((path, content))
}

#[cfg(test)]
#[path = "bundles/tests.rs"]
mod tests;
