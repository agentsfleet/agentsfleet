//! Retry broker subscriptions without turning each retry into another database assignment pass.

use crate::{Gateway, Operation, Result, error::ErrorKind};
use af_core::error_code::{Coded as _, Logged};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use tokio_util::sync::CancellationToken;

/// The longest wait between two subscription repairs, when the deployment's
/// `[runners] max_repair_backoff_seconds` names no other.
pub(crate) const REPAIR_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(5);

pub(super) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let wake = &gateway.inner.dependencies.wake;
    let mut retry = std::time::Duration::from_millis(100);
    let ceiling = gateway.inner.dependencies.recovery.max_repair_backoff;
    let mut lost = false;
    loop {
        let listening = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            result = tokio::time::timeout(STREAM_ANSWER_TIMEOUT, wake.listen(&gateway.inner.dependencies.bus, &stop)) => result,
        };
        match listening {
            Ok(Ok(mut workers)) => {
                retry = std::time::Duration::from_millis(100);
                lost = false;
                wake.recheck();
                tokio::select! {
                    biased;
                    () = stop.cancelled() => { workers.shutdown().await; return Ok(()); }
                    _closed = workers.join_next() => { workers.shutdown().await; }
                }
            }
            Ok(Err(error)) => {
                let error_code = error.code().as_str();
                tracing::warn!(
                    event = "runner_stream_wake_subscriptions_failed",
                    error_code
                );
            }
            Err(source) => timed_out(source),
        }
        // One outage transition requests one pass.
        if !lost {
            wake.recheck();
            lost = true;
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            () = tokio::time::sleep(retry) => {}
        }
        retry = next_retry(retry, ceiling);
    }
}

/// The wait after `retry`: doubled, never past the deployment's `ceiling`.
fn next_retry(retry: std::time::Duration, ceiling: std::time::Duration) -> std::time::Duration {
    retry.saturating_mul(2).min(ceiling)
}

fn timed_out(source: tokio::time::error::Elapsed) {
    let failure: crate::Error = ErrorKind::Timeout {
        operation: Operation::Recheck,
        source,
    }
    .into();
    let Logged { error_code, reason } = failure.logged();
    tracing::warn!(
        event = "runner_stream_wake_subscriptions_timed_out",
        error_code,
        reason
    );
}

#[cfg(test)]
mod tests {
    use super::{REPAIR_BACKOFF_MAX, next_retry};
    use std::time::Duration;

    #[test]
    fn a_configured_ceiling_bounds_the_repair_backoff() {
        let ceiling = Duration::from_secs(2);
        assert_eq!(
            next_retry(Duration::from_millis(800), ceiling),
            Duration::from_millis(1_600)
        );
        assert_eq!(next_retry(Duration::from_millis(1_600), ceiling), ceiling);
        assert_eq!(
            next_retry(Duration::from_secs(4), REPAIR_BACKOFF_MAX),
            Duration::from_secs(5)
        );
        assert_eq!(
            crate::RecoverySettings::default().max_repair_backoff,
            REPAIR_BACKOFF_MAX
        );
    }
}
