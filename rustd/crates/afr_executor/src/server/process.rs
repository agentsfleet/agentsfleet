//! One process, from start to its single `process/exited`.
//!
//! The task that drives a process owns everything about it — its exit, its
//! output channel, its edges — and is reached only through its control
//! channel. Nothing it waits on can stall it: input is written by a task of
//! its own, and output is drained for a bounded grace once the leader ends.
//! When the control channel closes, because the session ended, the process is
//! stopped exactly as a kill would stop it.

use std::time::Duration;

use bytes::Bytes;
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::sync::{mpsc, oneshot};
use tokio_util::task::AbortOnDropHandle;

use super::launch::{Exit, Input, Plan, Spawned, launcher};
use super::session::post;
use crate::api::Ending;
use crate::edges::{Chunk, EDGE_BYTES, OutputEdges};
use crate::error::{self, Result};
use crate::protocol::{ExitedParams, NOTIFY_EXITED, NOTIFY_OUTPUT, OutputParams, encode, line};

/// How long a process's group has between TERM and KILL.
pub(crate) const KILL_GRACE: Duration = Duration::from_secs(2);
/// How long output may keep arriving once the leader has ended. A descendant
/// that left the group — `setsid cmd &`, a background job on a terminal —
/// can hold the output open indefinitely; past this it is not waited for.
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(2);
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
/// A process's input closed; later writes are refused.
const EVENT_INPUT_CLOSED: &str = "executor_input_closed";
/// A signal found no process group, usually because it had already ended.
const EVENT_SIGNAL_MISSED: &str = "executor_signal_missed";

/// What the session asks of a running process, with where to answer.
pub(super) enum Control {
    /// Queue bytes for its input; answered once queued.
    Write(Bytes, oneshot::Sender<Result<()>>),
    /// Stop its group; acknowledged as soon as stopping begins.
    Kill(oneshot::Sender<()>),
}

/// A write or a kill, before it has somewhere to answer.
pub(super) enum Steer {
    /// Queue these bytes for its input.
    Write(Bytes),
    /// Stop its group.
    Kill,
}

impl Steer {
    /// Hands this to the process behind `control` and waits for its answer.
    pub(super) async fn deliver(self, control: &mpsc::Sender<Control>) -> Result<()> {
        match self {
            Self::Write(data) => ask(control, |reply| Control::Write(data, reply)).await?,
            Self::Kill => ask(control, Control::Kill).await,
        }
    }
}

/// Sends the message `make` builds and waits for the reply it carries.
async fn ask<T>(
    control: &mpsc::Sender<Control>,
    make: impl FnOnce(oneshot::Sender<T>) -> Control,
) -> Result<T> {
    let (reply, answer) = oneshot::channel();
    control
        .send(make(reply))
        .await
        .map_err(|_ended| error::unknown_process())?;
    answer.await.map_err(|_ended| error::unknown_process())
}

/// A started process and when its executor stops waiting for it.
pub(super) struct ProcessRun {
    spawned: Spawned,
    timeout: Option<Duration>,
}

impl ProcessRun {
    /// Starts what `plan` describes.
    pub(super) fn start(plan: &Plan) -> Result<Self> {
        let spawned = launcher(plan.on_terminal()).launch(plan)?;
        Ok(Self {
            spawned,
            timeout: plan.time_limit(),
        })
    }

    /// Forwards output, serves writes and kills, enforces the timeout, and
    /// sends the one `process/exited`; answers with the process's number.
    pub(super) async fn drive(
        self,
        process: u64,
        mut controls: mpsc::Receiver<Control>,
        outbound: mpsc::UnboundedSender<String>,
    ) -> u64 {
        let Spawned {
            pid,
            input,
            mut exit,
            mut output,
            readers,
        } = self.spawned;
        let event = EVENT_PROCESS_STARTED;
        tracing::debug!(event, process, "a process started");
        let (writes, queued) = mpsc::channel(INPUT_BACKLOG);
        let _writer = AbortOnDropHandle::new(tokio::spawn(feed_input(input, queued, process)));
        let mut edges = OutputEdges::new(EDGE_BYTES);
        let mut emit = |chunk: Chunk| post(&outbound, output_line(process, &chunk));
        let deadline = deadline(self.timeout);
        tokio::pin!(deadline);
        let ending = loop {
            tokio::select! {
                Some(chunk) = output.recv() => edges.feed(chunk, &mut emit),
                control = controls.recv() => match control {
                    Some(Control::Write(data, reply)) => {
                        let _caller_gone = reply.send(queue(&writes, data));
                    }
                    Some(Control::Kill(acknowledge)) => {
                        let _caller_gone = acknowledge.send(());
                        break stop(pid, &mut exit).await;
                    }
                    None => break stop(pid, &mut exit).await,
                },
                () = &mut deadline => {
                    stop(pid, &mut exit).await;
                    break Ending::TimedOut;
                }
                ending = &mut exit => break ending,
            }
        };
        // Whatever the leader left in its group goes with it, so the output
        // closes and the drain below ends; a descendant outside the group is
        // waited for only as long as the grace.
        signal(pid, Signal::KILL);
        let drained = tokio::time::timeout(DRAIN_GRACE, async {
            while let Some(chunk) = output.recv().await {
                edges.feed(chunk, &mut emit);
            }
        })
        .await;
        if drained.is_err() {
            let event = EVENT_OUTPUT_ABANDONED;
            tracing::debug!(event, process, "output still open past the drain grace");
        }
        drop(readers);
        let omitted_bytes = edges.finish(&mut emit);
        let exited = ExitedParams {
            process_id: process,
            ending,
            omitted_bytes,
        };
        post(&outbound, line(&notification(NOTIFY_EXITED, exited)));
        report(process, ending, omitted_bytes);
        process
    }
}

/// Logs how a process ended: completed with a status, or failed without one.
fn report(process: u64, ending: Ending, omitted_bytes: u64) {
    if ending == Ending::Interrupted {
        let event = EVENT_PROCESS_FAILED;
        let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            event,
            error_code,
            process,
            "how a process ended could not be learned"
        );
    } else {
        let event = EVENT_PROCESS_COMPLETED;
        let ending = format!("{ending:?}");
        tracing::debug!(event, process, ending, omitted_bytes, "a process ended");
    }
}

/// Queues `data` for the input writer without waiting on it.
fn queue(writes: &mpsc::Sender<Bytes>, data: Bytes) -> Result<()> {
    writes.try_send(data).map_err(|refused| match refused {
        mpsc::error::TrySendError::Full(_) => error::input_backlog_full(),
        mpsc::error::TrySendError::Closed(_) => {
            std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()
        }
    })
}

/// Writes queued input in order until the queue closes or the input does.
async fn feed_input(mut input: Box<dyn Input>, mut queued: mpsc::Receiver<Bytes>, process: u64) {
    while let Some(data) = queued.recv().await {
        if let Err(failure) = input.write(data).await {
            let event = EVENT_INPUT_CLOSED;
            let reason = failure.to_string();
            tracing::debug!(event, process, reason, "a process's input closed");
            break;
        }
    }
}

/// Resolves once `limit` elapses, or never.
async fn deadline(limit: Option<Duration>) {
    match limit {
        Some(limit) => tokio::time::sleep(limit).await,
        None => std::future::pending().await,
    }
}

/// TERM to the group, a grace, then KILL to the group.
async fn stop(pid: Pid, exit: &mut Exit) -> Ending {
    signal(pid, Signal::TERM);
    match tokio::time::timeout(KILL_GRACE, &mut *exit).await {
        Ok(ending) => ending,
        Err(_elapsed) => {
            signal(pid, Signal::KILL);
            exit.await
        }
    }
}

/// Signals `pid`'s whole group; a group already gone is expected.
fn signal(pid: Pid, signal: Signal) {
    if let Err(errno) = kill_process_group(pid, signal) {
        let event = EVENT_SIGNAL_MISSED;
        let reason = errno.to_string();
        tracing::debug!(event, reason, "a signal found no process group");
    }
}

/// The `process/output` line for one chunk.
fn output_line(process: u64, chunk: &Chunk) -> String {
    let output = OutputParams {
        process_id: process,
        stream: chunk.stream,
        data: encode(&chunk.data),
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
