//! Bounded sandbox commands retain definite endings and kill on cancellation.

use afr_executor::{Ending, Executor, ProcessEvent, Spawn, Stream};
use afr_tools::{ToolErrorCode, ToolOutput};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    executor: &dyn Executor,
    spawn: Spawn,
    timeout: Duration,
    stop: &CancellationToken,
    limit: usize,
    machine: bool,
) -> Result<Vec<u8>, ToolOutput> {
    let mut process = executor.spawn(&spawn.timeout(timeout)).await.map_err(|_| {
        ToolOutput::failed(
            ToolErrorCode::SandboxUnavailable,
            "The validation workspace is unavailable.",
        )
    })?;
    let deadline = tokio::time::Instant::now() + timeout + Duration::from_secs(10);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut incomplete = false;
    loop {
        let event = tokio::select! {
            biased;
            () = stop.cancelled() => None,
            () = tokio::time::sleep_until(deadline) => None,
            event = process.events.recv() => event,
        };
        match event {
            Some(ProcessEvent::Output { stream, data }) => {
                let target = if stream == Stream::Stdout {
                    &mut stdout
                } else {
                    &mut stderr
                };
                let remaining = limit.saturating_sub(target.len());
                target.extend_from_slice(&data[..data.len().min(remaining)]);
                incomplete |= data.len() > remaining;
            }
            Some(ProcessEvent::Omitted { .. }) => incomplete = true,
            Some(ProcessEvent::Ended {
                ending: Ending::Exited(0),
                output_abandoned,
            }) if !machine || !incomplete && !output_abandoned => {
                if incomplete || output_abandoned {
                    stdout.extend_from_slice(b"\n[output truncated]");
                }
                return Ok(stdout);
            }
            Some(ProcessEvent::Ended { ending, .. }) => {
                if machine {
                    return Err(ToolOutput::failed(
                        ToolErrorCode::AgentsfleetdRefused,
                        "Candidate capture was incomplete or refused an unsupported workspace.",
                    ));
                }
                return Err(ToolOutput::failed(
                    ToolErrorCode::AgentsfleetdRefused,
                    &format!(
                        "Validation or candidate capture was not confirmed ({ending:?}).\n{}\n{}",
                        String::from_utf8_lossy(&stdout[..stdout.len().min(16 * 1024)]),
                        String::from_utf8_lossy(&stderr[..stderr.len().min(16 * 1024)])
                    ),
                ));
            }
            None => {
                let _killed = executor.kill(process.id).await;
                return Err(ToolOutput::failed(
                    ToolErrorCode::Interrupted,
                    "Validation ended without a complete result; publication was not requested.",
                ));
            }
        }
    }
}
