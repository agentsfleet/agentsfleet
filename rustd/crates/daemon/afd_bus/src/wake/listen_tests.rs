//! Shard notifications reach every replica's queue, and the listeners release
//! their subscriptions once they stop.

use std::{collections::BTreeSet, time::Duration};

use af_core::id::Uuid7;
use tokio_util::sync::CancellationToken;

use super::{SHARDS, Wake, publish};
use crate::{Subject, test_util};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = Duration::from_secs(5);

/// Two replicas on one bus: both queues hear every fleet, a malformed
/// notification is skipped, and stopping the listeners frees every shard.
#[tokio::test]
async fn sharded_bus_notifications_reach_each_replica_and_release_their_listeners() -> TestResult {
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
    bus.publish(&Subject::Wake(0), "malformed fleet").await?;
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

/// Every shard has exactly `wanted` readers.
fn assert_readers(server: &test_util::Server, wanted: usize) -> TestResult {
    for shard in 0..SHARDS {
        let readers = server.readers(&Subject::Wake(shard));
        if readers != wanted {
            return Err(format!("shard {shard} has {readers} readers, wanted {wanted}").into());
        }
    }
    Ok(())
}
