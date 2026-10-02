//! One process, from start to its single `process/exited`.
//!
//! The task that drives a process owns everything about it — its input, its
//! exit, its output channel, its edges — and is reached only through its
//! control channel. When that channel closes, because the session ended, the
//! process is stopped exactly as a kill would stop it.

use std::time::Duration;

use bytes::Bytes;
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::sync::{mpsc, oneshot};

use super::launch::{Exit, Plan, Spawned, Status, launcher};
use super::session::post;
use crate::edges::{Chunk, EDGE_BYTES, OutputEdges};
use crate::error::{self, Result};
use crate::protocol::{ExitedParams, NOTIFY_EXITED, NOTIFY_OUTPUT, OutputParams, encode, line};

/// How long a process's group has between TERM and KILL.
pub(crate) const KILL_GRACE: Duration = Duration::from_secs(2);
/// A process started.
const EVENT_PROCESS_STARTED: &str = "executor_process_started";
/// A process ended and its last output was sent.
const EVENT_PROCESS_ENDED: &str = "executor_process_ended";
/// A signal found no process group, usually because it had already ended.
const EVENT_SIGNAL_MISSED: &str = "executor_signal_missed";

/// What the session asks of a running process, with where to answer.
pub(super) enum Control {
    /// Write to its input.
    Write(Bytes, oneshot::Sender<std::io::Result<()>>),
    /// Stop its group; acknowledged as soon as stopping begins.
    Kill(oneshot::Sender<()>),
}

/// A write or a kill, before it has somewhere to answer.
pub(super) enum Steer {
    /// Write these bytes to its input.
    Write(Bytes),
    /// Stop its group.
    Kill,
}

impl Steer {
    /// Hands this to the process behind `control` and waits for its answer.
    pub(super) async fn deliver(self, control: &mpsc::Sender<Control>) -> Result<()> {
        match self {
            Self::Write(data) => Ok(ask(control, |reply| Control::Write(data, reply)).await??),
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
            mut input,
            mut exit,
            mut output,
        } = self.spawned;
        let event = EVENT_PROCESS_STARTED;
        tracing::debug!(event, process, "a process started");
        let mut edges = OutputEdges::new(EDGE_BYTES);
        let mut emit = |chunk: Chunk| post(&outbound, output_line(process, &chunk));
        let deadline = deadline(self.timeout);
        tokio::pin!(deadline);
        let mut timed_out = false;
        let status = loop {
            tokio::select! {
                Some(chunk) = output.recv() => edges.feed(chunk, &mut emit),
                control = controls.recv() => match control {
                    Some(Control::Write(data, reply)) => {
                        let _caller_gone = reply.send(input.write(data).await);
                    }
                    Some(Control::Kill(acknowledge)) => {
                        let _caller_gone = acknowledge.send(());
                        break stop(pid, &mut exit).await;
                    }
                    None => break stop(pid, &mut exit).await,
                },
                () = &mut deadline => {
                    timed_out = true;
                    break stop(pid, &mut exit).await;
                }
                status = &mut exit => break status,
            }
        };
        // Whatever the leader left in its group goes with it, so the output
        // pipes close and the drain below ends.
        signal(pid, Signal::KILL);
        while let Some(chunk) = output.recv().await {
            edges.feed(chunk, &mut emit);
        }
        let omitted_bytes = edges.finish(&mut emit);
        let exited = ExitedParams {
            process_id: process,
            exit_code: status.code(),
            signal: status.signal(),
            timed_out,
            omitted_bytes,
        };
        post(&outbound, line(&notification(NOTIFY_EXITED, exited)));
        let event = EVENT_PROCESS_ENDED;
        tracing::debug!(event, process, timed_out, omitted_bytes, "a process ended");
        process
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
async fn stop(pid: Pid, exit: &mut Exit) -> Status {
    signal(pid, Signal::TERM);
    match tokio::time::timeout(KILL_GRACE, &mut *exit).await {
        Ok(status) => status,
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
        stream: chunk.stream.into(),
        data: encode(&chunk.data),
    };
    line(&notification(NOTIFY_OUTPUT, output))
}

/// A notification envelope.
fn notification<T>(method: &'static str, params: T) -> jsonrpsee_types::Notification<'static, T> {
    jsonrpsee_types::Notification::new(method.into(), params)
}
