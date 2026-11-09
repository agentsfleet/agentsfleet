//! The live adapter delivers shard notifications to independent replica queues.

use std::{collections::BTreeSet, time::Duration};

use af_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use afd_dragonfly::{
    Dragonfly,
    config::{DragonflyConfig, DragonflyRole},
};
use tokio_util::sync::CancellationToken;

use super::{SHARDS, Wake, publish};
use crate::{Subject, dragonfly, test_util::physical_channel};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = Duration::from_secs(5);

fn config() -> TestResult<DragonflyConfig> {
    let url = ProcessEnv
        .get("TEST_DRAGONFLY_URL")
        .ok_or("TEST_DRAGONFLY_URL is unset; use make test-integration-rustd")?;
    Ok(DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get("TEST_DRAGONFLY_CA_CERT").map(Into::into))
        .with_request_timeout(BOUND))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn sharded_bus_notifications_reach_each_replica_and_release_their_listeners() -> TestResult {
    let config = config()?;
    let client = afd_dragonfly::test_util::connect_live(&config).await?;
    let buses = [
        dragonfly::connect(client.clone(), config.clone()).await?,
        dragonfly::connect(client.clone(), config).await?,
    ];
    let wakes = [Wake::default(), Wake::default()];
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let mut listeners = [
        wakes[0].listen(&buses[0], &stop).await?,
        wakes[1].listen(&buses[1], &stop).await?,
    ];
    let channels: Vec<_> = (0..SHARDS)
        .map(|shard| physical_channel(&Subject::Wake(shard)))
        .collect();
    subscribers(&client, &channels, 2).await?;
    let expected: BTreeSet<_> = (0..24)
        .map(|index| Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}")))
        .collect::<af_core::error::Result<_>>()?;
    buses[0]
        .publish(&Subject::Wake(0), "malformed fleet")
        .await?;
    for fleet in &expected {
        publish(&buses[0], fleet.clone()).await;
    }
    receive(&wakes, &expected).await?;
    stop.cancel();
    for workers in &mut listeners {
        while let Some(done) = workers.join_next().await {
            done?;
        }
    }
    for bus in &buses {
        bus.shutdown().await?;
    }
    subscribers(&client, &channels, 0).await?;
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

async fn subscribers(client: &Dragonfly, channels: &[String], wanted: i64) -> TestResult {
    tokio::time::timeout(BOUND, async {
        loop {
            let mut ready = true;
            for channel in channels {
                ready &= count(client, channel).await? == wanted;
            }
            if ready {
                return TestResult::Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

async fn count(client: &Dragonfly, channel: &str) -> TestResult<i64> {
    let mut command = redis::cmd("PUBSUB");
    command.arg("NUMSUB").arg(channel);
    let reply: Vec<redis::Value> = client.command("PUBSUB", channel, &command).await?;
    Ok(reply
        .into_iter()
        .filter_map(|value| match value {
            redis::Value::Map(entries) => Some(entries),
            _ => None,
        })
        .flatten()
        .filter_map(|(_, value)| match value {
            redis::Value::Int(count) => Some(count),
            _ => None,
        })
        .sum())
}
