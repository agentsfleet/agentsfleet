//! Carrier completion preserves its queued final frame while cancellation releases pending reads.

use super::*;
use bytes::BytesMut;
use tokio_quiche::http3::driver::InboundFrame;

#[tokio::test]
async fn a_buffered_final_frame_survives_carrier_cancellation()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (sender, frames) = tokio::sync::mpsc::channel(1);
    let stop = CancellationToken::new();
    let mut body = Incoming::h3(frames, stop.clone());
    let payload = br#"{"type":"go_away"}"#;
    sender
        .send(InboundFrame::Body(BytesMut::from(payload.as_slice()), true))
        .await?;
    stop.cancel();
    let frame = body
        .frame()
        .await
        .transpose()?
        .ok_or("final frame was lost")?;
    assert_eq!(frame.data_ref(), Some(&Bytes::from_static(payload)));
    assert!(body.frame().await.is_none());
    Ok(())
}

#[tokio::test]
async fn cancellation_releases_a_body_waiting_for_native_data() -> Result<()> {
    let (_sender, frames) = tokio::sync::mpsc::channel(1);
    let stop = CancellationToken::new();
    let mut body = Incoming::h3(frames, stop.clone());
    stop.cancel();
    let failure = body
        .frame()
        .await
        .transpose()
        .err()
        .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), crate::Reason::Cancelled);
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(body.frame().await.is_none());
    Ok(())
}
