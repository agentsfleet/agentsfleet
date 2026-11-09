//! What one fleet's tail turns each thing a subscription reports into.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::id::Uuid7;
use afd_bus::{Subject, test_util};

type TestResult = crate::Result<(), Box<dyn std::error::Error + Send + Sync>>;
use futures_util::StreamExt as _;

use super::tail;
use crate::frame::{Frame, KIND_CATCHING_UP};

const FLEET: &str = "0195b4ba-8d3a-7000-8abc-000000000001";
const CHUNK: &str = r#"{"kind":"chunk","text":"hi"}"#;

/// Frames past a reader's buffer, which is 256 deep: these many are lost.
const OVERFLOW: u64 = 44;

async fn next(stream: &mut (impl futures_util::Stream<Item = Frame> + Unpin)) -> Frame {
    stream.next().await.expect("the tail stays open")
}

/// A subscription lost and restored reaches the client as `catching_up` with
/// nothing counted — nobody saw what was missed — and spends no sequence
/// number, so the ids stay gapless over what the client received.
#[tokio::test]
async fn a_gap_is_catching_up_with_nothing_counted_and_no_sequence_spent() -> TestResult {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::FleetTail(Uuid7::parse(FLEET)?);
    let mut frames = Box::pin(tail(bus.subscribe(&subject).await?));
    server.gap(&subject);
    server.publish(&subject, CHUNK);
    assert_eq!(next(&mut frames).await.seq, 0);

    server.gap(&subject);
    let gap = next(&mut frames).await;
    assert_eq!((gap.seq, gap.kind.as_ref()), (0, KIND_CATCHING_UP));
    assert_eq!(gap.data, r#"{"kind":"catching_up","dropped":0}"#);

    server.publish(&subject, CHUNK);
    assert_eq!(next(&mut frames).await.seq, 1, "the gap spent no id");
    Ok(())
}

/// A reader that fell behind is told how many frames it lost, then resumes
/// at the next id.
#[tokio::test]
async fn a_lag_is_catching_up_with_the_count_it_lost() -> TestResult {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::FleetTail(Uuid7::parse(FLEET)?);
    let mut frames = Box::pin(tail(bus.subscribe(&subject).await?));
    for _ in 0..(256 + OVERFLOW) {
        server.publish(&subject, CHUNK);
    }
    let lag = next(&mut frames).await;
    assert_eq!(lag.kind, KIND_CATCHING_UP);
    assert_eq!(
        lag.data,
        format!(r#"{{"kind":"catching_up","dropped":{OVERFLOW}}}"#)
    );
    assert_eq!(next(&mut frames).await.seq, 0, "the lag spent no id");
    Ok(())
}

/// A hub shut down ends the tail rather than leaving it waiting.
#[tokio::test]
async fn a_closed_hub_ends_the_tail() -> TestResult {
    let (bus, _server) = test_util::detached()?;
    let subject = Subject::FleetTail(Uuid7::parse(FLEET)?);
    let mut frames = Box::pin(tail(bus.subscribe(&subject).await?));
    bus.shutdown().await?;
    assert!(frames.next().await.is_none());
    Ok(())
}

/// The line a closed tail leaves names the registry code in its own
/// `error_code` field and says why without repeating it, or a backtrace.
#[tokio::test]
async fn a_closed_tail_reason_carries_no_code_prefix() -> TestResult {
    let capture = af_core::test_util::trace::Capture::install();
    let (bus, _server) = test_util::detached()?;
    let subject = Subject::FleetTail(Uuid7::parse(FLEET)?);
    let mut frames = Box::pin(tail(bus.subscribe(&subject).await?));
    bus.shutdown().await?;
    assert!(frames.next().await.is_none());

    let logged = capture.only("sse_tail_closed");
    logged.assert_uncoded_reason();
    assert!(
        logged
            .field("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    Ok(())
}
