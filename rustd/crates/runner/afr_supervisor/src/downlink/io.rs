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
