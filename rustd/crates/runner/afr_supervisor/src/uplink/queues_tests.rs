//! Each stream kind has its own outbound queue, and a closed progress queue is
//! a lost session rather than a silently dropped frame.

use super::queues::{Outbound, Queues};
use crate::error::ErrorKind;
use af_wire::protocol::StreamKind;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

type Proof = Result<(), Box<dyn std::error::Error + Send + Sync>>;

async fn outbound() -> af_stream::Result<Outbound> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let frame = af_stream::encode(
        &serde_json::json!({"type":"ping", "request_id":1}),
        &af_stream::Budget::default(),
        StreamKind::Progress,
        &CancellationToken::new(),
        deadline,
    )
    .await?;
    Ok(Outbound { frame, deadline })
}

#[tokio::test]
async fn progress_rides_its_own_queue_and_a_closed_queue_is_a_disconnection() -> Proof {
    let (queues, mut receivers) = Queues::new();
    queues
        .send(
            StreamKind::Progress,
            false,
            outbound().await?,
            &CancellationToken::new(),
        )
        .await?;
    receivers.progress.try_recv()?;
    assert!(
        receivers.work.try_recv().is_err(),
        "no other lane carried it"
    );

    drop(receivers);
    let lost = queues.progress(outbound().await?);
    assert!(lost.is_err_and(|error| matches!(error.kind(), ErrorKind::Disconnected)));
    Ok(())
}
