//! Thread wakes reach every replica's queue through one wildcard
//! subscription, and the listener releases it once it stops.

use std::{collections::BTreeSet, time::Duration};

use af_core::{error_code::INTERNAL_OPERATION_FAILED, id::Uuid7, test_util::trace::Capture};
use tokio_util::sync::CancellationToken;

use super::{Recovery, Wake, publish};
use crate::{Subject, test_util};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = Duration::from_secs(5);

/// Two replicas on one bus: both queues hear every thread, a malformed
/// notification is skipped, and stopping the listeners frees the wildcard.
#[tokio::test]
async fn thread_wakes_reach_each_replica_and_release_their_listeners() -> TestResult {
    let (bus, server) = test_util::detached()?;
    let wakes = [Wake::default(), Wake::default()];
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let mut listeners = [
        wakes[0].listen(&bus, &stop).await?,
        wakes[1].listen(&bus, &stop).await?,
    ];
    assert_readers(&server, 2)?;
    let expected: BTreeSet<_> = (0..24)
        .map(|index| Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}")))
        .collect::<af_core::error::Result<_>>()?;
    let malformed = Subject::ThreadWake(Uuid7::parse("0195b4ba-8d3a-7000-8abc-0000000000ff")?);
    bus.publish(&malformed, "malformed fleet").await?;
    for fleet in &expected {
        publish(&bus, fleet.clone()).await;
    }
    receive(&wakes, &expected).await?;
    stop.cancel();
    for workers in &mut listeners {
        while let Some(done) = workers.join_next().await {
            done?;
        }
    }
    bus.shutdown().await?;
    assert_readers(&server, 0)?;
    Ok(())
}

/// A gap or an overrun on the wildcard loses an unknown set of threads, so
/// each asks for a full recheck, and a thread that still arrives is queued.
#[tokio::test]
async fn a_gap_or_a_lag_on_the_wildcard_asks_for_a_full_recheck() -> TestResult {
    let (bus, server) = test_util::detached()?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let _listeners = wake.listen(&bus, &stop).await?;
    assert_eq!(wake.recovery(), None, "nothing is lost yet");

    let fleet = Uuid7::parse("0195b4ba-8d3a-7000-8abc-000000000007")?;
    server.gap(&Subject::ThreadWake(fleet.clone()));
    recovered(&wake, Recovery(1)).await?;

    // The reader runs on this thread, so it cannot drain while these queue:
    // it falls behind its channel and is told how many it lost.
    let overrun = crate::nats::Tuning::default().channel_capacity + 1;
    for _ in 0..overrun {
        server.publish(&Subject::ThreadWake(fleet.clone()), fleet.as_str());
    }
    recovered(&wake, Recovery(2)).await?;
    tokio::time::timeout(BOUND, async {
        while wake.fleets().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(wake.take(), Some(fleet), "what did arrive is still queued");
    Ok(())
}

/// A listener whose subscription closes stops, says so, and asks for a
/// recheck so nothing waits on a notification that will never come.
#[tokio::test]
async fn a_closed_subscription_stops_its_listener_after_asking_for_a_recheck() -> TestResult {
    let (bus, _server) = test_util::detached()?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let mut listeners = wake.listen(&bus, &stop).await?;
    let capture = Capture::install();

    bus.shutdown().await?;
    let mut stopped = 0;
    tokio::time::timeout(BOUND, async {
        while let Some(done) = listeners.join_next().await {
            done?;
            stopped += 1;
        }
        Ok::<_, tokio::task::JoinError>(())
    })
    .await??;

    assert!(!stop.is_cancelled(), "the listener stopped on its own");
    assert_eq!(stopped, 1, "one listener reads every thread");
    assert_eq!(wake.recovery(), Some(Recovery(1)), "it asked once");
    let closed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("turn_wake_subscription_closed"))
        .collect();
    assert_eq!(closed.len(), 1);
    for event in closed {
        assert_eq!(event.level, tracing::Level::WARN);
        assert_eq!(
            event.field("error_code"),
            Some(INTERNAL_OPERATION_FAILED.as_str())
        );
    }
    Ok(())
}

/// Waits until `wake` asks for exactly `wanted`.
async fn recovered(wake: &Wake, wanted: Recovery) -> TestResult {
    tokio::time::timeout(BOUND, async {
        while wake.recovery() != Some(wanted) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

async fn receive(wakes: &[Wake; 2], expected: &BTreeSet<Uuid7>) -> TestResult {
    let mut received = [BTreeSet::new(), BTreeSet::new()];
    tokio::time::timeout(BOUND, async {
        loop {
            for (wake, seen) in wakes.iter().zip(&mut received) {
                while let Some(fleet) = wake.take() {
                    if expected.contains(&fleet) {
                        seen.insert(fleet);
                    }
                }
            }
            if received.iter().all(|seen| seen == expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

/// A wake on any thread reaches exactly `wanted` readers.
fn assert_readers(server: &test_util::Server, wanted: usize) -> TestResult {
    let thread = Uuid7::parse("0195b4ba-8d3a-7000-8abc-0000000000aa")?;
    let readers = server.readers(&Subject::ThreadWake(thread));
    if readers != wanted {
        return Err(format!("a thread's wake has {readers} readers, wanted {wanted}").into());
    }
    Ok(())
}
