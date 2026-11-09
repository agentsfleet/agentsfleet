//! Retry broker subscriptions without turning each retry into another database claim pass.

use crate::{Gateway, Result};
use af_core::timing::{OFFER_FALLBACK_TICK, STREAM_ANSWER_TIMEOUT};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(gateway: Gateway, stop: CancellationToken) -> Result<()> {
    let wake = &gateway.inner.dependencies.wake;
    let mut retry = std::time::Duration::from_millis(100);
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
            Err(_) => tracing::warn!(event = "runner_stream_wake_subscriptions_timed_out"),
        }
        // One outage transition requests one pass. The independent five-second probe covers the outage.
        if !lost {
            wake.recheck();
            lost = true;
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            () = tokio::time::sleep(retry) => {}
        }
        retry = retry.saturating_mul(2).min(OFFER_FALLBACK_TICK);
    }
}
