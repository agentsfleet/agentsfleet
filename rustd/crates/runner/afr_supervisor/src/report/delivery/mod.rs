//! The worker owns retry state; PostgreSQL owns persisted completed reports.

use crate::{Result, client::endless, error::ErrorKind, halt::Halt};

const EVENT_WAITING: &str = "runner_result_waiting";

/// `None` is an acknowledgement that settlement is still pending. A worker
/// keeps its report and capacity until `Some` confirms the completed write.
pub(crate) async fn until_acknowledged<T, F, Fut>(halt: &Halt, mut call: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    let mut pauses = endless();
    loop {
        let answer = tokio::select! {
            biased;
            () = halt.stopped().cancelled() => return Err(ErrorKind::Disconnected.into()),
            answer = call() => answer,
        };
        match answer {
            Ok(Some(response)) => return Ok(response),
            Ok(None) => {
                let event = EVENT_WAITING;
                tracing::debug!(event, reason = "settlement_pending");
            }
            Err(failure) => {
                if !failure.is_retryable() {
                    halt.stops_on(&failure);
                    return Err(failure);
                }
                let error_code = failure.code().as_str();
                let event = EVENT_WAITING;
                tracing::warn!(event, error_code);
            }
        }
        let pause = pauses.next().ok_or(ErrorKind::StreamProtocol)?;
        tokio::select! {
            biased;
            () = halt.stopped().cancelled() => return Err(ErrorKind::Disconnected.into()),
            () = tokio::time::sleep(pause) => {},
        }
    }
}

#[cfg(test)]
mod tests;
