//! Real bounded I/O proves framing, deadlines and automatic memory release.

use std::{error::Error as _, time::Duration};

use af_stream::{Budget, Reader, Reason, write};
use af_wire::protocol::{Lane, MESSAGE_BYTES_MAX, Up};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt as _},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use super::native::Result;

#[derive(Default)]
struct Fixture {
    budget: Budget,
    stop: CancellationToken,
}

impl Fixture {
    fn reader<R: AsyncRead + Unpin>(&self, input: R, lane: Lane) -> Reader<R> {
        Reader::new(
            input,
            self.budget.clone(),
            lane,
            self.stop.clone(),
            Duration::from_secs(5),
        )
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    async fn encode<T: serde::Serialize + Sync>(
        &self,
        value: &T,
        lane: Lane,
    ) -> af_stream::Result<af_stream::OwnedFrame> {
        af_stream::encode(value, &self.budget, lane, &self.stop, Self::deadline()).await
    }
}

async fn send_raw(output: &mut (impl AsyncWrite + Unpin), payload: &[u8]) -> Result {
    output
        .write_all(&u32::try_from(payload.len())?.to_be_bytes())
        .await?;
    output.write_all(payload).await?;
    Ok(())
}

#[tokio::test]
async fn split_frames_and_consecutive_frames_preserve_boundaries() -> Result {
    let fixture = Fixture::default();
    let available = fixture.budget.available(Lane::Control);
    let (mut output, input) = tokio::io::duplex(1);
    let sending = async {
        for payload in [
            br#"{"type":"ping","request_id":1}"#,
            br#"{"type":"ping","request_id":2}"#,
        ] {
            send_raw(&mut output, payload).await?;
        }
        output.shutdown().await?;
        Result::Ok(())
    };
    let receiving = async {
        let mut reader = fixture.reader(input, Lane::Control);
        for expected in [1, 2] {
            let frame = reader.next().await?.ok_or("ping frame missing")?;
            let ping: Up<'_> = frame.decode()?;
            assert_eq!(
                ping.request_id().map(std::num::NonZeroU64::get),
                Some(expected)
            );
        }
        assert!(reader.next().await?.is_none());
        Result::Ok(())
    };
    tokio::try_join!(sending, receiving)?;
    assert_eq!(fixture.budget.available(Lane::Control), available);
    Ok(())
}

#[tokio::test]
async fn oversized_prefix_is_refused_before_waiting_for_memory_or_payload() -> Result {
    let fixture = Fixture {
        budget: Budget::new(1, 1)?,
        ..Fixture::default()
    };
    let (mut output, input) = tokio::io::duplex(4);
    output.write_all(&u32::MAX.to_be_bytes()).await?;
    let Err(error) = fixture.reader(input, Lane::Reports).next().await else {
        return Err("oversized prefix accepted".into());
    };
    assert_eq!(error.reason(), Reason::Protocol);
    assert_eq!(fixture.budget.available(Lane::Reports), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn partial_header_times_out_and_returns_its_memory() -> Result {
    let fixture = Fixture::default();
    let available = fixture.budget.available(Lane::Control);
    let (mut output, input) = tokio::io::duplex(4);
    output.write_all(&[0]).await?;
    let Err(error) = fixture.reader(input, Lane::Control).next().await else {
        return Err("partial header accepted".into());
    };
    assert_eq!(error.reason(), Reason::Timeout);
    assert_eq!(fixture.budget.available(Lane::Control), available);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn partial_payload_times_out_and_releases_its_reservation() -> Result {
    let fixture = Fixture::default();
    let available = fixture.budget.available(Lane::Reports);
    let (mut output, input) = tokio::io::duplex(8);
    output.write_all(&20_u32.to_be_bytes()).await?;
    output.write_all(b"{").await?;
    let Err(error) = fixture.reader(input, Lane::Reports).next().await else {
        return Err("partial payload accepted".into());
    };
    assert_eq!(error.reason(), Reason::Timeout);
    assert_eq!(fixture.budget.available(Lane::Reports), available);
    Ok(())
}

#[tokio::test]
async fn malformed_json_preserves_its_cause_without_exposing_the_payload() -> Result {
    let fixture = Fixture::default();
    let (mut output, input) = tokio::io::duplex(64);
    send_raw(&mut output, b"private-fixture-text").await?;
    let frame = fixture
        .reader(input, Lane::Control)
        .next()
        .await?
        .ok_or("frame missing")?;
    let Err(error) = frame.decode::<Up<'_>>() else {
        return Err("invalid JSON accepted".into());
    };
    assert_eq!(error.reason(), Reason::Protocol);
    let cause = error.source().ok_or("JSON cause missing")?;
    assert_ne!(error.to_string(), cause.to_string());
    assert!(!format!("{frame:?}").contains("private-fixture-text"));
    Ok(())
}

#[tokio::test]
async fn data_pressure_preserves_control_memory_and_cancellation_releases_waiters() -> Result {
    let fixture = Fixture {
        budget: Budget::new(MESSAGE_BYTES_MAX * 2, MESSAGE_BYTES_MAX * 2)?,
        ..Fixture::default()
    };
    let reason = {
        let value = "x".repeat(MESSAGE_BYTES_MAX - 2);
        let _held = fixture.encode(&value, Lane::Reports).await?;
        let waiting = fixture.encode(&"waiting", Lane::Reports);
        tokio::pin!(waiting);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                waiting.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        let _control = fixture.encode(&"ping", Lane::Control).await?;
        fixture.stop.cancel();
        let Err(error) = waiting.await else {
            return Err("cancelled memory waiter succeeded".into());
        };
        error.reason()
    };
    assert_eq!(reason, Reason::Cancelled);
    assert_eq!(
        fixture.budget.available(Lane::Control),
        MESSAGE_BYTES_MAX * 2
    );
    assert_eq!(
        fixture.budget.available(Lane::Reports),
        MESSAGE_BYTES_MAX * 2
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_blocked_write_obeys_the_whole_frame_deadline() -> Result {
    let fixture = Fixture::default();
    let frame = fixture.encode(&"ping", Lane::Control).await?;
    let (mut output, _unread) = tokio::io::duplex(1);
    let Err(error) = write(&mut output, &frame, &fixture.stop, Fixture::deadline()).await else {
        return Err("blocked write succeeded".into());
    };
    assert_eq!(error.reason(), Reason::Timeout);
    Ok(())
}

#[tokio::test]
async fn maximum_payload_round_trips_without_crossing_its_allocation_bound() -> Result {
    let fixture = Fixture::default();
    let value = "x".repeat(MESSAGE_BYTES_MAX - 2);
    let frame = fixture.encode(&value, Lane::Reports).await?;
    assert_eq!(frame.as_bytes().len(), MESSAGE_BYTES_MAX);
    let (mut output, input) = tokio::io::duplex(1024);
    let mut reader = fixture.reader(input, Lane::Reports);
    let ((), received) = tokio::try_join!(
        write(&mut output, &frame, &fixture.stop, Fixture::deadline()),
        reader.next()
    )?;
    let received = received.ok_or("payload frame missing")?;
    assert_eq!(received.decode::<String>()?, value);
    let oversized = "x".repeat(MESSAGE_BYTES_MAX - 1);
    let Err(error) = fixture.encode(&oversized, Lane::Reports).await else {
        return Err("oversized frame encoded".into());
    };
    assert_eq!(error.reason(), Reason::Protocol);
    Ok(())
}
