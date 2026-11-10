//! Native stream tasks share one connection owner and bounded frame queues.

use super::connect::Connected;
use crate::{
    Result,
    error::ErrorKind,
    uplink::{StreamHandle, queues::Receivers, writer},
};
use af_stream::{OwnedFrame, Reader};
use af_wire::protocol::StreamKind;
use tokio::{sync::mpsc, task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument as _, instrument::WithSubscriber as _};

pub(super) fn spawn(
    connected: Connected,
    receivers: Receivers,
    control_frames: mpsc::Sender<(StreamKind, OwnedFrame)>,
    frames: &mpsc::Sender<(StreamKind, OwnedFrame)>,
    stop: &CancellationToken,
    workers: &mut JoinSet<Result<()>>,
) -> af_stream::Connection {
    let Connected {
        connection,
        control,
        work,
        reports,
        activity,
    } = connected;
    write_pipes(
        control.writer,
        work.writer,
        reports.writer,
        activity.writer,
        receivers,
        stop,
        workers,
    );
    track(
        workers,
        read(
            control.reader,
            StreamKind::Control,
            control_frames,
            stop.clone(),
        ),
    );
    for (kind, pipe) in [
        (StreamKind::Work, work.reader),
        (StreamKind::Reports, reports.reader),
        (StreamKind::Activity, activity.reader),
    ] {
        track(workers, read(pipe, kind, frames.clone(), stop.clone()));
    }
    connection
}

fn write_pipes(
    control: tokio::io::DuplexStream,
    work: tokio::io::DuplexStream,
    reports: tokio::io::DuplexStream,
    activity: tokio::io::DuplexStream,
    receivers: Receivers,
    stop: &CancellationToken,
    workers: &mut JoinSet<Result<()>>,
) {
    let Receivers {
        priority,
        control: ordinary,
        work: work_rx,
        reports: reports_rx,
        activity: activity_rx,
    } = receivers;
    track(
        workers,
        writer::control(control, priority, ordinary, stop.clone()),
    );
    track(workers, writer::run(work, work_rx, stop.clone()));
    track(workers, writer::run(reports, reports_rx, stop.clone()));
    track(workers, writer::run(activity, activity_rx, stop.clone()));
}

async fn read(
    mut reader: Reader<af_stream::IncomingReader>,
    kind: StreamKind,
    frames: mpsc::Sender<(StreamKind, OwnedFrame)>,
    stop: CancellationToken,
) -> Result<()> {
    loop {
        let frame = reader.next().await?.ok_or(ErrorKind::Disconnected)?;
        tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            sent = frames.send((kind, frame)) => sent.map_err(crate::error::stream_disconnected)?,
        }
    }
}

pub(super) async fn ping(handle: StreamHandle, stop: CancellationToken) -> Result<()> {
    let mut ticks = tokio::time::interval_at(
        Instant::now() + af_core::timing::STREAM_PING_INTERVAL,
        af_core::timing::STREAM_PING_INTERVAL,
    );
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! { () = stop.cancelled() => return Ok(()), _ = ticks.tick() => {} }
        handle.compute(crate::client::stream::Ping).await?;
    }
}

/// Native codec tasks carry their connection's correlation and scoped subscriber.
pub(super) fn track(
    workers: &mut JoinSet<Result<()>>,
    task: impl Future<Output = Result<()>> + Send + 'static,
) {
    workers.spawn(task.in_current_span().with_current_subscriber());
}

/// Join a closed session's tasks within one answer bound, then abort any that ignored
/// cancellation, so a stuck task can never hold the runner away from reconnecting.
pub(super) async fn join(mut workers: JoinSet<Result<()>>) {
    let joined = tokio::time::timeout(af_core::timing::STREAM_ANSWER_TIMEOUT, async {
        while let Some(done) = workers.join_next().await {
            observe(done);
        }
    })
    .await;
    if joined.is_err() {
        let remaining = workers.len();
        tracing::warn!(event = "runner_stream_workers_aborted", remaining);
        workers.shutdown().await;
    }
}

fn observe(done: std::result::Result<Result<()>, tokio::task::JoinError>) {
    match done {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let error_code = error.code().as_str();
            tracing::debug!(event = "runner_stream_worker_ended", error_code);
        }
        Err(error) => {
            let is_panic = error.is_panic();
            let is_cancelled = error.is_cancelled();
            let failure: crate::Error = error.into();
            let error_code = failure.code().as_str();
            tracing::warn!(
                event = "runner_stream_worker_failed",
                error_code,
                is_panic,
                is_cancelled
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_task_that_ignores_cancellation_is_aborted_after_one_answer_bound() {
        let mut workers = JoinSet::new();
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = std::sync::Arc::clone(&finished);
        workers.spawn(async move {
            std::future::pending::<()>().await;
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
        workers.spawn(async { Ok(()) });
        let started = Instant::now();
        join(workers).await;
        assert_eq!(started.elapsed(), af_core::timing::STREAM_ANSWER_TIMEOUT);
        assert!(!finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn finished_tasks_join_without_waiting_for_the_bound() {
        let mut workers = JoinSet::new();
        workers.spawn(async { Err(ErrorKind::Disconnected.into()) });
        workers.spawn(async { Ok(()) });
        let started = Instant::now();
        join(workers).await;
        assert_eq!(started.elapsed(), std::time::Duration::ZERO);
    }
}
