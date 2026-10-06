//! One process, from start to its single `process/exited`.
//!
//! The task that drives a process owns everything about it — its exit and its
//! output channel — and is reached only through its stop token; its input
//! goes straight from the session to the launcher's writer. Every byte of
//! output is forwarded, as it arrives; the supervisor's side bounds what its
//! reader has not read. The one thing it waits on is the connection's
//! bounded output queue, when a process writes faster than the supervisor
//! reads: the process then waits on its own full pipe. Once the leader ends,
//! what it left is drained, bounded by a grace waiting on the pipe and a
//! byte cap. When the token is cancelled, by a
//! kill or because the session ended, the process is stopped the same way.

use std::time::Duration;

use bytes::Bytes;
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::launch::{Exit, OUTPUT_BACKLOG, Plan, READ_CHUNK_BYTES, Spawned, launcher};
use crate::api::Ending;
use crate::edges::Chunk;
use crate::error::Result;
use crate::protocol::{ExitedParams, NOTIFY_EXITED, NOTIFY_OUTPUT, OutputParams, line};

/// How long a process's group has between TERM and KILL.
pub(crate) const KILL_GRACE: Duration = Duration::from_secs(2);
/// How long the drain waits on the pipe once the leader has ended. A
/// descendant that left the group — `setsid cmd &`, a background job on a
/// terminal — can hold the output open indefinitely; past this it is not
/// waited for. Time spent waiting on the writer does not count: that is the
/// supervisor reading, not the process holding its output open.
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// The most output forwarded once the leader has ended: twice what the
/// reader's backlog holds, room for that and a pipe's buffer. Past it, a
/// descendant outside the group that keeps writing is not forwarded.
const DRAIN_BYTES_MAX: usize = 2 * OUTPUT_BACKLOG * READ_CHUNK_BYTES;
/// Writes that may wait for a process to read its input; past this a write
/// is refused rather than held.
pub(crate) const INPUT_BACKLOG: usize = 16;
/// A process started.
const EVENT_PROCESS_STARTED: &str = "executor_process_started";
/// A process ended with a status, and its last output was sent.
const EVENT_PROCESS_COMPLETED: &str = "executor_process_completed";
/// A process ended but how could not be learned.
const EVENT_PROCESS_FAILED: &str = "executor_process_failed";
/// Output was still open past the drain grace and was left behind.
const EVENT_OUTPUT_ABANDONED: &str = "executor_output_abandoned";
/// A signal found no process group, usually because it had already ended.
const EVENT_SIGNAL_MISSED: &str = "executor_signal_missed";

/// A started process and when its executor stops waiting for it.
pub(super) struct ProcessRun {
    spawned: Spawned,
    timeout: Option<Duration>,
}

impl ProcessRun {
    /// Starts what `plan` describes, and answers with where its input goes.
    pub(super) fn start(plan: &Plan) -> Result<(Self, mpsc::Sender<Bytes>)> {
        let (input, queued) = mpsc::channel(INPUT_BACKLOG);
        let spawned = launcher(plan.on_terminal()).launch(plan, queued)?;
        let run = Self {
            spawned,
            timeout: plan.time_limit(),
        };
        Ok((run, input))
    }

    /// Forwards output to `lines`, enforces the timeout, stops the process
    /// when `stop` is cancelled, and sends the one `process/exited` after the
    /// last output; answers with the process's number.
    pub(super) async fn drive(
        self,
        process: u64,
        stop: CancellationToken,
        lines: mpsc::Sender<Bytes>,
    ) -> u64 {
        let Spawned {
            pid,
            mut exit,
            mut output,
            tasks,
        } = self.spawned;
        let group = Group { process, pid };
        let event = EVENT_PROCESS_STARTED;
        tracing::debug!(event, process_id = process, "a process started");
        let deadline = deadline(self.timeout);
        tokio::pin!(deadline);
        let ending = loop {
            tokio::select! {
                Some(chunk) = output.recv() => forward(&lines, process, chunk).await,
                () = stop.cancelled() => break group.stop(&mut exit).await,
                () = &mut deadline => {
                    group.stop(&mut exit).await;
                    break Ending::TimedOut;
                }
                ending = &mut exit => break ending,
            }
        };
        // Ended, so a write or a kill from here on finds no process — before
        // the supervisor can hear that it ended and ask.
        stop.cancel();
        // Whatever the leader left in its group goes with it, so the output
        // closes and the drain below ends; a descendant outside the group is
        // waited for only as far as the grace and the cap.
        group.signal(Signal::KILL);
        let output_abandoned = !drain(&mut output, &lines, process).await;
        if output_abandoned {
            let event = EVENT_OUTPUT_ABANDONED;
            tracing::debug!(
                event,
                process_id = process,
                "output still open past the drain's grace or its cap"
            );
        }
        drop(tasks);
        let exited = ExitedParams {
            process_id: process,
            ending,
            output_abandoned,
        };
        let _writer_gone = lines.send(line(&notification(NOTIFY_EXITED, exited))).await;
        report(process, ending);
        process
    }
}

/// Forwards what is left in `output` once the leader has ended, until it
/// closes; `false` when it was given up on, past the grace spent waiting on
/// the pipe or past the byte cap.
async fn drain(
    output: &mut mpsc::Receiver<Chunk>,
    lines: &mpsc::Sender<Bytes>,
    process: u64,
) -> bool {
    let mut grace = DRAIN_GRACE;
    let mut forwarded = 0;
    while forwarded < DRAIN_BYTES_MAX {
        let waiting = Instant::now();
        let Ok(next) = tokio::time::timeout(grace, output.recv()).await else {
            return false;
        };
        let Some(chunk) = next else {
            return true;
        };
        grace = grace.saturating_sub(waiting.elapsed());
        forwarded += chunk.data.len();
        forward(lines, process, chunk).await;
    }
    false
}

/// Hands one chunk to the writer, waiting while it is behind. A writer that
/// has ended means the supervisor is gone, and there is no one left to tell.
async fn forward(lines: &mpsc::Sender<Bytes>, process: u64, chunk: Chunk) {
    let _writer_gone = lines.send(output_line(process, chunk)).await;
}

/// A process's group, named by its leader, and the number the session knows
/// the process by.
struct Group {
    process: u64,
    pid: Pid,
}

impl Group {
    /// TERM to the group, a grace, then KILL to the group.
    async fn stop(&self, exit: &mut Exit) -> Ending {
        self.signal(Signal::TERM);
        match tokio::time::timeout(KILL_GRACE, &mut *exit).await {
            Ok(ending) => ending,
            Err(_elapsed) => {
                self.signal(Signal::KILL);
                exit.await
            }
        }
    }

    /// Signals the whole group; a group already gone is expected.
    fn signal(&self, signal: Signal) {
        if let Err(errno) = kill_process_group(self.pid, signal) {
            let event = EVENT_SIGNAL_MISSED;
            tracing::debug!(
                event,
                process_id = self.process,
                reason = %errno,
                "a signal found no process group"
            );
        }
    }
}

/// Logs how a process ended: completed with a status, or failed without one.
fn report(process: u64, ending: Ending) {
    if ending == Ending::Interrupted {
        let event = EVENT_PROCESS_FAILED;
        let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            event,
            error_code,
            process_id = process,
            "how a process ended could not be learned"
        );
    } else {
        let event = EVENT_PROCESS_COMPLETED;
        let (ending, code) = (ending.kind(), ending.code());
        tracing::debug!(event, process_id = process, ending, code, "a process ended");
    }
}

/// Resolves once `limit` elapses, or never.
async fn deadline(limit: Option<Duration>) {
    match limit {
        Some(limit) => tokio::time::sleep(limit).await,
        None => std::future::pending().await,
    }
}

/// The `process/output` line for one chunk.
fn output_line(process: u64, chunk: Chunk) -> Bytes {
    let output = OutputParams {
        process_id: process,
        stream: chunk.stream,
        data: chunk.data,
    };
    line(&notification(NOTIFY_OUTPUT, output))
}

/// A notification envelope.
fn notification<T>(method: &'static str, params: T) -> jsonrpsee_types::Notification<'static, T> {
    jsonrpsee_types::Notification::new(method.into(), params)
}

#[cfg(test)]
#[path = "process/tests.rs"]
mod tests;
