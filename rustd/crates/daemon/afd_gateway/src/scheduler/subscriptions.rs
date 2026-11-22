//! Retry broker subscriptions without turning each retry into another database assignment pass.

use crate::{Gateway, Operation, error::ErrorKind};
use af_core::error_code::{Coded as _, Logged};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use backon::{BackoffBuilder as _, ExponentialBuilder};
use tokio_util::sync::CancellationToken;

/// The longest wait between two subscription repairs, when the deployment's
/// `[runners] max_repair_backoff_seconds` names no other.
pub(crate) const REPAIR_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(5);

pub(super) async fn run(gateway: Gateway, stop: CancellationToken) {
    let wake = &gateway.inner.dependencies.wake;
    let schedule = repair_schedule(gateway.inner.dependencies.recovery.max_repair_backoff);
    let mut pauses = schedule.build();
    let mut lost = false;
    loop {
        let listening = tokio::select! {
            biased;
            () = stop.cancelled() => return,
            result = tokio::time::timeout(STREAM_ANSWER_TIMEOUT, wake.listen(&gateway.inner.dependencies.bus, &stop)) => result,
        };
        match listening {
            Ok(Ok(mut workers)) => {
                pauses = schedule.build();
                lost = false;
                wake.recheck();
                tokio::select! {
                    biased;
                    () = stop.cancelled() => { workers.shutdown().await; return; }
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
            () = stop.cancelled() => return,
            // The schedule has no last attempt, so it always has a wait.
            () = tokio::time::sleep(pauses.next().unwrap_or_default()) => {}
        }
    }
}

/// Repair waits: 100 ms, doubling to the deployment's `ceiling`, for as long
/// as the scheduler runs.
const fn repair_schedule(ceiling: std::time::Duration) -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_factor(2.0)
        .with_min_delay(std::time::Duration::from_millis(100))
        .with_max_delay(ceiling)
        .without_max_times()
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
    use super::{REPAIR_BACKOFF_MAX, repair_schedule, run};
    use backon::BackoffBuilder as _;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    #[tokio::test(start_paused = true)]
    async fn a_lost_broker_asks_for_one_recovery_pass_however_long_it_stays_lost() -> TestResult {
        let wake = afd_bus::wake::Wake::default();
        let gateway = crate::fixture::gateway(
            afd_db::test_util::unreachable_db(),
            afd_bus::test_util::unreachable(),
            wake.clone(),
        )?;
        let capture = af_core::test_util::trace::Capture::install();
        let stop = CancellationToken::new();
        let repairing = tokio::spawn(run(gateway, stop.clone()));
        tokio::time::sleep(Duration::from_millis(1)).await;
        let first = wake.recovery().ok_or("the lost broker asked for no pass")?;
        // Every repair attempt over a whole backoff ceiling fails again.
        tokio::time::sleep(REPAIR_BACKOFF_MAX * 4).await;
        assert_eq!(
            wake.recovery(),
            Some(first),
            "each failed repair asked for another pass"
        );
        stop.cancel();
        repairing.await?;
        let failures: Vec<_> = capture
            .events()
            .into_iter()
            .filter(|event| event.field("event") == Some("runner_stream_wake_subscriptions_failed"))
            .collect();
        assert!(
            failures.len() > 1,
            "every failed repair is logged: {failures:?}"
        );
        assert!(
            failures
                .iter()
                .all(|failure| failure.level == tracing::Level::WARN
                    && failure.field("error_code")
                        == Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str()))
        );
        Ok(())
    }

    #[test]
    fn a_configured_ceiling_bounds_the_repair_backoff() {
        // Backon doubles in `f32` seconds, so a wait is whole milliseconds
        // plus a few nanoseconds.
        let waits: Vec<_> = repair_schedule(Duration::from_secs(2))
            .build()
            .take(7)
            .map(|wait| wait.as_millis())
            .collect();
        assert_eq!(waits, [100, 200, 400, 800, 1_600, 2_000, 2_000]);
    }
}
