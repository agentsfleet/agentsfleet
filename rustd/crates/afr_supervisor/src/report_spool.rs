//! A finished run's report, on disk before its first post.
//!
//! The report is the only record of what a run did, so it is never held only
//! in memory: [`ReportSpool::hold`] writes it to a temporary file, syncs it,
//! and renames it into place before anything is posted. The entry goes only on
//! an answer that settles the lease for good; anything a later attempt could
//! change keeps it, and the drain posts it again.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::PathBuf;

use afd_core::error_code::{
    ErrorCode, RUN_LEASE_LOST, RUN_LEASE_NOT_FOUND, RUN_STALE_FENCING_TOKEN,
};
use afd_core::id::Uuid7;
use bytes::Bytes;
use tempfile::NamedTempFile;

use crate::client::ControlPlane;
use crate::error::{Error, Result};
use crate::storage_home::StorageHome;

/// The extension every spooled report carries.
const EXTENSION: &str = "json";
/// The extension a report the daemon cannot read is moved aside under.
const REJECTED: &str = "rejected";
/// Refusals that settle the lease for good: the lease is already settled, or
/// was handed to another runner. No retry would change them.
const SETTLED_BY: [ErrorCode; 3] = [RUN_STALE_FENCING_TOKEN, RUN_LEASE_NOT_FOUND, RUN_LEASE_LOST];
/// Refusal statuses a later attempt can change: a token or a grant restored, a
/// timeout, a size limit raised, a rate limit lifted.
const KEPT_ON: [u16; 5] = [401, 403, 408, 413, 429];
const EVENT_DELIVERED: &str = "report_spool_delivered";
const EVENT_SUPERSEDED: &str = "report_spool_superseded";
const EVENT_QUARANTINED: &str = "report_spool_quarantined";

/// The directory reports wait in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReportSpool {
    dir: PathBuf,
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
        Self { dir: home.spool() }
    }

    /// Writes a report's `bytes` durably, before anything posts them.
    pub(crate) fn hold(&self, lease_id: &Uuid7, bytes: Bytes) -> Result<Spooled> {
        let mut file = NamedTempFile::new_in(&self.dir)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        let path = self.dir.join(lease_id.as_str()).with_extension(EXTENSION);
        file.persist(&path)?;
        // The rename is durable only once the directory entry is.
        File::open(&self.dir)?.sync_all()?;
        Ok(Spooled { path, bytes })
    }

    /// Every report still waiting for an answer that settles it.
    pub(crate) fn pending(&self) -> Result<Vec<Spooled>> {
        let mut pending = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
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
}

impl Spooled {
    /// Posts the report once, and settles the entry by the daemon's answer.
    ///
    /// # Errors
    /// An entry that cannot be removed or moved aside.
    pub(crate) async fn deliver(&self, plane: &ControlPlane) -> Result<Delivery> {
        let answer = plane.report(self.bytes.clone()).await;
        let delivery = match answer {
            Ok(()) => Delivery::Settled,
            Err(failure) if kept(&failure) => return Ok(Delivery::Kept(failure)),
            Err(refusal) if settles(&refusal) => {
                let code = refusal.code().as_str();
                let event = EVENT_SUPERSEDED;
                tracing::warn!(
                    error_code = code,
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
                    event,
                    "the daemon cannot read this report; it is kept aside"
                );
                fs::rename(&self.path, self.path.with_extension(REJECTED))?;
                return Ok(Delivery::Rejected);
            }
        };
        // Another delivery of the same entry may have removed it first.
        match fs::remove_file(&self.path) {
            Err(failure) if failure.kind() != io::ErrorKind::NotFound => return Err(failure.into()),
            _gone => {}
        }
        let event = EVENT_DELIVERED;
        tracing::info!(event);
        Ok(delivery)
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
