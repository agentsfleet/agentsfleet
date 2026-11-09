//! Each stream has one writer; only control chooses between priority and ordinary frames.

use tokio::{io::DuplexStream, sync::mpsc};
use tokio_util::sync::CancellationToken;

use super::queues::Outbound;
use crate::Result;

pub(crate) async fn run(
    mut writer: DuplexStream,
    mut frames: mpsc::Receiver<Outbound>,
    stop: CancellationToken,
) -> Result<()> {
    loop {
        let message = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            message = frames.recv() => message,
        };
        let Some(message) = message else {
            return Ok(());
        };
        af_stream::write(&mut writer, &message.frame, &stop, message.deadline).await?;
    }
}

pub(crate) async fn control(
    mut writer: DuplexStream,
    mut priority: mpsc::Receiver<Outbound>,
    mut ordinary: mpsc::Receiver<Outbound>,
    stop: CancellationToken,
) -> Result<()> {
    loop {
        let message = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            message = priority.recv() => message,
            message = ordinary.recv() => message,
        };
        let Some(message) = message else {
            return Ok(());
        };
        af_stream::write(&mut writer, &message.frame, &stop, message.deadline).await?;
    }
}
