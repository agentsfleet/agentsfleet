//! Two file operations share one transfer pool across every worker on this runner.

use super::transfer::Transfers;
use crate::{Result, error};
use afr_agent::FileRecoveryPermit;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) struct RecoveryResources {
    pub(crate) transfers: Transfers,
    files: Arc<Semaphore>,
}

impl RecoveryResources {
    #[cfg(test)]
    pub(crate) fn new() -> Result<Self> {
        Self::configured(crate::config::RuntimePolicy::DEFAULT)
    }

    pub(crate) fn configured(policy: crate::config::RuntimePolicy) -> Result<Self> {
        Ok(Self {
            transfers: Transfers::new()?,
            files: Arc::new(Semaphore::new(
                policy.settings().max_concurrent_file_operations,
            )),
        })
    }

    pub(crate) fn try_file(&self) -> Option<FileRecoveryPermit> {
        Arc::clone(&self.files)
            .try_acquire_owned()
            .ok()
            .map(Arc::new)
    }

    pub(crate) async fn file(&self, stop: &CancellationToken) -> Result<FileRecoveryPermit> {
        tokio::select! {
            biased;
            () = stop.cancelled() => Err(error::config("file recovery stopped while waiting for capacity")),
            permit = Arc::clone(&self.files).acquire_owned() => {
                permit
                    .map(Arc::new)
                    .map_err(|source| error::boundary("file recovery admission closed", source))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Poll;

    #[tokio::test]
    async fn configured_single_file_slot_is_shared_until_its_last_owner_releases() -> Result<()> {
        let settings = garde::Unvalidated::new(crate::config::RuntimeSettings {
            max_concurrent_file_operations: 1,
            ..crate::config::RuntimeSettings::default()
        })
        .validate()?;
        let resources =
            RecoveryResources::configured(crate::config::RuntimePolicy::new(&settings))?;
        let first = resources.file(&CancellationToken::new()).await?;
        let writer = Arc::clone(&first);
        drop(first);
        assert!(resources.try_file().is_none());
        drop(writer);
        assert!(resources.try_file().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn required_work_queues_and_optional_work_skips_a_busy_runner() -> Result<()> {
        let resources = RecoveryResources::new()?;
        let stop = CancellationToken::new();
        let first = resources.file(&stop).await?;
        let second = resources.file(&stop).await?;
        let writer = Arc::clone(&first);
        drop(first);
        let queued = resources.file(&stop);
        tokio::pin!(queued);
        assert!(matches!(futures_util::poll!(&mut queued), Poll::Pending));
        assert!(resources.try_file().is_none());

        let cancelled = CancellationToken::new();
        let aborted = resources.file(&cancelled);
        tokio::pin!(aborted);
        assert!(matches!(futures_util::poll!(&mut aborted), Poll::Pending));
        cancelled.cancel();
        let Err(refusal) = aborted.await else {
            return Err(error::config("cancelled admission must refuse queued work"));
        };
        assert_eq!(
            refusal.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert!(matches!(
            refusal.kind(),
            error::ErrorKind::Config { detail }
                if *detail == "file recovery stopped while waiting for capacity"
        ));

        drop(writer);
        let next = queued.await?;
        assert!(resources.try_file().is_none());
        drop((second, next));
        assert!(resources.try_file().is_some());
        Ok(())
    }
}
