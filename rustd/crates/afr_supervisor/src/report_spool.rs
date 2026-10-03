//! A finished run's report, on disk before its first post.
//!
//! The report is the only record of what a run did, so it is never held only
//! in memory: [`ReportSpool::hold`] writes it to a temporary file, syncs it,
//! and renames it into place before anything is posted. The entry goes only on
//! an answer that settles the lease for good; anything a later attempt could
//! change keeps it, and the drain posts it again.
//!
//! Writing and reading the spool syncs and walks the disk, so both run on the
//! blocking pool, never on a worker that renewals and heartbeats share.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use afd_core::error_code::{
    ErrorCode, RUN_LEASE_LOST, RUN_LEASE_NOT_FOUND, RUN_STALE_FENCING_TOKEN,
};
use afd_core::id::Uuid7;
use bytes::Bytes;
use reqwest::StatusCode;
use tempfile::NamedTempFile;

use crate::client::ControlPlane;
use crate::error::{Error, Result, UNAUTHORIZED};
use crate::storage_home::StorageHome;

/// The extension every spooled report carries.
const EXTENSION: &str = "json";
/// The extension a report the daemon cannot read is moved aside under.
const REJECTED: &str = "rejected";
/// Refusals that settle the lease for good: the lease is already settled, or
/// was handed to another runner. No retry would change them.
const SETTLED_BY: [ErrorCode; 3] = [RUN_STALE_FENCING_TOKEN, RUN_LEASE_NOT_FOUND, RUN_LEASE_LOST];
/// Refusal statuses a later attempt can change: a token or a grant restored, a
/// timeout, a size limit raised. A rate limit is not among them: the client
/// reads a 429 as unavailable, which is retried before it is ever a refusal.
const KEPT_ON: [u16; 4] = [
    UNAUTHORIZED,
    StatusCode::FORBIDDEN.as_u16(),
    StatusCode::REQUEST_TIMEOUT.as_u16(),
    StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
];
const EVENT_DELIVERED: &str = "report_spool_delivered";
const EVENT_SUPERSEDED: &str = "report_spool_superseded";
const EVENT_QUARANTINED: &str = "report_spool_quarantined";

/// The directory reports wait in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReportSpool {
    dir: Arc<Path>,
}

/// One report on disk, with the bytes that will be posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spooled {
    path: PathBuf,
    bytes: Bytes,
}

/// What became of one delivery attempt.
#[derive(Debug)]
pub(crate) enum Delivery {
    /// The daemon settled the lease, or answered that it already was; the
    /// entry is gone.
    Settled,
    /// The answer could change on a later attempt; the entry stays.
    Kept(Error),
    /// The daemon cannot read this report and never will; the entry is moved
    /// aside for an operator rather than retried forever.
    Rejected,
}

impl ReportSpool {
    /// The spool under `home`.
    pub(crate) fn new(home: &StorageHome) -> Self {
        Self {
            dir: home.spool().into(),
        }
    }

    /// Writes a report's `bytes` durably, before anything posts them.
    pub(crate) async fn hold(&self, lease_id: &Uuid7, bytes: Bytes) -> Result<Spooled> {
        let dir = Arc::clone(&self.dir);
        let path = dir.join(lease_id.as_str()).with_extension(EXTENSION);
        tokio::task::spawn_blocking(move || {
            write_durably(&dir, &path, &bytes)?;
            Ok(Spooled { path, bytes })
        })
        .await?
    }

    /// Every report still waiting for an answer that settles it.
    pub(crate) async fn pending(&self) -> Result<Vec<Spooled>> {
        let dir = Arc::clone(&self.dir);
        tokio::task::spawn_blocking(move || read_pending(&dir)).await?
    }
}

/// Writes `bytes` to a temporary file in `dir`, syncs it, and renames it to
/// `path`, then syncs the directory: the rename is durable only once the
/// directory entry is.
fn write_durably(dir: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    File::open(dir)?.sync_all()?;
    Ok(())
}

/// Every spooled report in `dir`.
fn read_pending(dir: &Path) -> Result<Vec<Spooled>> {
    let mut pending = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == EXTENSION)
        {
            let bytes = Bytes::from(fs::read(&path)?);
            pending.push(Spooled { path, bytes });
        }
    }
    Ok(pending)
}

impl Spooled {
    /// Posts the report once, and settles the entry by the daemon's answer.
    ///
    /// # Errors
    /// An entry that cannot be removed or moved aside.
    pub(crate) async fn deliver(&self, plane: &ControlPlane) -> Result<Delivery> {
        let answer = plane.report(self.bytes.clone()).await;
        let lease_id = self.lease_id();
        let delivery = match answer {
            Ok(()) => Delivery::Settled,
            Err(failure) if kept(&failure) => return Ok(Delivery::Kept(failure)),
            Err(refusal) if settles(&refusal) => {
                let code = refusal.code().as_str();
                let event = EVENT_SUPERSEDED;
                tracing::warn!(
                    error_code = code,
                    lease_id,
                    event,
                    "the daemon had already settled this lease"
                );
                Delivery::Settled
            }
            Err(refusal) => {
                let code = refusal.code().as_str();
                let event = EVENT_QUARANTINED;
                tracing::error!(
                    error_code = code,
                    lease_id,
                    event,
                    "the daemon cannot read this report; it is kept aside"
                );
                tokio::fs::rename(&self.path, self.path.with_extension(REJECTED)).await?;
                return Ok(Delivery::Rejected);
            }
        };
        // Another delivery of the same entry may have removed it first.
        match tokio::fs::remove_file(&self.path).await {
            Err(failure) if failure.kind() != io::ErrorKind::NotFound => return Err(failure.into()),
            _gone => {}
        }
        let event = EVENT_DELIVERED;
        tracing::info!(lease_id, event);
        Ok(delivery)
    }

    /// The lease the report settles, as its file is named; read here rather
    /// than kept, so the drain names a report a dead process left too.
    fn lease_id(&self) -> &str {
        self.path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
    }
}

/// Whether a later attempt could get a different answer.
fn kept(failure: &Error) -> bool {
    failure.is_retryable()
        || failure
            .refusal_status()
            .is_some_and(|status| KEPT_ON.contains(&status))
}

/// Whether a refusal means the lease is settled for good.
fn settles(refusal: &Error) -> bool {
    refusal
        .refusal_code()
        .is_some_and(|code| SETTLED_BY.contains(&code))
}

#[cfg(test)]
#[path = "report_spool/tests.rs"]
mod tests;
