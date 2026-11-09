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
    let Receivers {
        priority,
        control: ordinary,
        work: work_rx,
        reports: reports_rx,
        activity: activity_rx,
    } = receivers;
    workers.spawn(writer::control(
        control.writer,
        priority,
        ordinary,
        stop.clone(),
    ));
    workers.spawn(writer::run(work.writer, work_rx, stop.clone()));
    workers.spawn(writer::run(reports.writer, reports_rx, stop.clone()));
    workers.spawn(writer::run(activity.writer, activity_rx, stop.clone()));
    workers.spawn(read(
        control.reader,
        StreamKind::Control,
        control_frames,
        stop.clone(),
    ));
    for (kind, pipe) in [
        (StreamKind::Work, work.reader),
        (StreamKind::Reports, reports.reader),
        (StreamKind::Activity, activity.reader),
    ] {
        workers.spawn(read(pipe, kind, frames.clone(), stop.clone()));
    }
    connection
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
