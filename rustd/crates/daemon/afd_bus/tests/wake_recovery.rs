//! Listener loss accounting is isolated because metric producers are process-wide.
#![cfg(feature = "test-util")]

use std::time::Duration;

use af_core::id::Uuid7;
use af_observability::test_util::Capture;
use afd_bus::{Subject, test_util, wake::Wake};
use tokio_util::sync::CancellationToken;

type TestResult = afd_bus::Result<(), Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = Duration::from_secs(1);
const FAILURES: &str = "agentsfleet_fleet_ready_wake_failures_total";
const LABELS: [(&str, &str); 1] = [("kind", "gap")];

#[tokio::test]
async fn listener_recovers_from_malformed_gap_and_lag_and_releases_every_subscription() -> TestResult
{
    let capture = Capture::install();
    let before = capture.sum(FAILURES, &LABELS);
    let (bus, server) = test_util::detached()?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let mut listeners = wake.listen(&bus, &stop).await?;
    for shard in 0..afd_bus::wake::SHARDS {
        assert_eq!(server.readers(&Subject::Wake(shard)), 1);
    }

    prove_recovery(&wake, &server, &capture, before).await?;

    stop.cancel();
    tokio::time::timeout(BOUND, async {
        while let Some(done) = listeners.join_next().await {
            done?;
        }
        TestResult::Ok(())
    })
    .await??;
    for shard in 0..afd_bus::wake::SHARDS {
        assert_eq!(server.readers(&Subject::Wake(shard)), 0);
    }
    assert_eq!(capture.sum(FAILURES, &LABELS), before + 3);
    bus.shutdown().await?;
    Ok(())
}

async fn prove_recovery(
    wake: &Wake,
    server: &test_util::Server,
    capture: &Capture,
    before: u64,
) -> TestResult {
    let subject = Subject::Wake(0);
    let fleet = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000f001")?;
    assert!(wake.recovery().is_none());
    server.publish(&subject, "malformed fleet");
    server.publish(&subject, fleet.as_str());
    expect_hints(wake, std::slice::from_ref(&fleet)).await?;
    assert_recovery(wake, capture, before + 1)?;

    server.gap(&subject);
    server.publish(&subject, fleet.as_str());
    expect_hints(wake, std::slice::from_ref(&fleet)).await?;
    assert_recovery(wake, capture, before + 2)?;

    // This current-thread test cannot poll the listener during the synchronous
    // burst, so the native 256-message broadcast must report a single lag.
    for _ in 0..513 {
        server.publish(&subject, fleet.as_str());
    }
    let next = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000f002")?;
    server.publish(&subject, next.as_str());
    expect_hints(wake, &[fleet, next]).await?;
    assert_recovery(wake, capture, before + 3)?;
    Ok(())
}

async fn expect_hints(wake: &Wake, expected: &[Uuid7]) -> TestResult {
    let last = expected
        .last()
        .ok_or("a recovery phase needs a valid final hint")?;
    tokio::time::timeout(BOUND, async {
        // Keep earlier duplicate hints buffered until the final marker proves
        // every preceding notification was handled by the listener.
        while !wake.fleets().contains(last) {
            tokio::task::yield_now().await;
        }
        for wanted in expected {
            assert_eq!(
                wake.take().as_ref(),
                Some(wanted),
                "no invalid or lost frame fabricates a hint"
            );
        }
        assert_eq!(wake.take(), None, "each valid hint is consumed once");
    })
    .await?;
    Ok(())
}

fn assert_recovery(wake: &Wake, capture: &Capture, wanted: u64) -> TestResult {
    assert_eq!(capture.sum(FAILURES, &LABELS), wanted);
    let recovery = wake
        .recovery()
        .ok_or("listener loss did not request recovery")?;
    wake.recovered(recovery);
    assert!(wake.recovery().is_none());
    Ok(())
}
