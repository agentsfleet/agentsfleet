//! A finished run's report, on disk before its first post.
//!
//! The report is the only record of what a run did, so it is never held only
//! in memory: [`ReportSpool::hold`] writes it to a temporary file, syncs it,
//! and renames it into place before anything is posted, and the entry goes
//! only once the daemon has answered for it. A runner killed between the two
//! finds the entry at boot and posts it then, once.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::PathBuf;

use afd_core::id::Uuid7;
use afd_wire::report::ReportRequest;
use bytes::Bytes;
use tempfile::NamedTempFile;

use crate::client::ControlPlane;
use crate::error::{self, Result};
use crate::storage_home::StorageHome;

/// The extension every spooled report carries.
const EXTENSION: &str = "json";
const EVENT_DELIVERED: &str = "report_spool_delivered";
const EVENT_SUPERSEDED: &str = "report_spool_superseded";

/// The directory reports wait in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSpool {
    dir: PathBuf,
}

/// One report on disk, with the bytes that will be posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spooled {
    path: PathBuf,
    bytes: Bytes,
}

/// What the daemon did with a delivered report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// It took the report; the entry is gone.
    Accepted,
    /// It refused the report for good — the lease was already settled or
    /// superseded — so the entry is gone too: no retry would change the answer.
    Refused,
}

impl ReportSpool {
    /// The spool under `home`.
    #[must_use]
    pub fn new(home: &StorageHome) -> Self {
        Self { dir: home.spool() }
    }

    /// Writes `report` durably, before anything posts it.
    ///
    /// # Errors
    /// A report that will not serialize, or a write, sync or rename that fails.
    pub fn hold(&self, lease_id: &Uuid7, report: &ReportRequest<'_>) -> Result<Spooled> {
        let bytes = serde_json::to_vec(report).map_err(error::encode)?;
        let mut file = NamedTempFile::new_in(&self.dir)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        let path = self.dir.join(lease_id.as_str()).with_extension(EXTENSION);
        file.persist(&path)?;
        // The rename is durable only once the directory entry is.
        File::open(&self.dir)?.sync_all()?;
        Ok(Spooled {
            path,
            bytes: Bytes::from(bytes),
        })
    }

    /// Every report a previous process spooled and never saw answered.
    ///
    /// # Errors
    /// A spool that cannot be listed, or an entry that cannot be read.
    pub fn pending(&self) -> Result<Vec<Spooled>> {
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
    /// Posts the report once, and removes the entry once the daemon answered.
    ///
    /// # Errors
    /// A retryable failure, which leaves the entry for the next attempt, or an
    /// entry that cannot be removed.
    pub async fn deliver(self, plane: &ControlPlane) -> Result<Delivery> {
        let delivery = match plane.report(self.bytes).await {
            Ok(()) => Delivery::Accepted,
            Err(failure) if failure.is_retryable() => return Err(failure),
            Err(refusal) => {
                let code = refusal.code().as_str();
                let event = EVENT_SUPERSEDED;
                tracing::warn!(
                    error_code = code,
                    event,
                    "the daemon refused a report for good"
                );
                Delivery::Refused
            }
        };
        fs::remove_file(&self.path)?;
        let event = EVENT_DELIVERED;
        tracing::info!(event);
        Ok(delivery)
    }
}

#[cfg(test)]
#[path = "report_spool/tests.rs"]
mod tests;
