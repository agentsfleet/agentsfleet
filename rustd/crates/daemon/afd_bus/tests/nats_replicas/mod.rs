//! Two replicas on different nodes: the wake ring reaches both, and a runner
//! command reaches only the runner it names.

use std::{collections::BTreeSet, time::Duration};

use af_core::id::Uuid7;
use afd_bus::{
    Command, Received, Subject,
    nats::Tuning,
    wake::{self, Wake},
};
use tokio_util::sync::CancellationToken;

use crate::support::{Fallible, PATIENCE, TestResult, bus, namespace, next, seeds, settle};

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn a_wake_reaches_each_replica_and_releases_its_listeners() -> TestResult {
    let (first, second) = replicas().await?;
    let wakes = [Wake::default(), Wake::default()];
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let mut listeners = [
        wakes[0].listen(&first, &stop).await?,
        wakes[1].listen(&second, &stop).await?,
    ];
    let expected: BTreeSet<_> = (0..24)
        .map(|index| Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}")))
        .collect::<af_core::error::Result<_>>()?;
    first.publish(&Subject::Wake(0), "malformed fleet").await?;
    receive(&first, &wakes, &expected).await?;
    stop.cancel();
    for workers in &mut listeners {
        while let Some(done) = workers.join_next().await {
            done?;
        }
    }
    first.shutdown().await?;
    second.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn a_runner_command_reaches_only_the_runner_it_names() -> TestResult {
    let (publishing, receiving) = replicas().await?;
    let runner = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c001")?;
    let addressed = Subject::RunnerCommand(runner.clone());
    let unrelated = Subject::RunnerCommand(Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c002")?);
    let mut recipient = receiving.subscribe(&addressed).await?;
    let mut other = receiving.subscribe(&unrelated).await?;
    settle(&publishing, &addressed, &mut [&mut recipient]).await?;
    settle(&publishing, &unrelated, &mut [&mut other]).await?;
    let command = Command::Cancel {
        lease_id: Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c003")?,
    };
    command.publish(&publishing, &runner).await?;
    let Received::Message(message) = next(&mut recipient).await? else {
        return Err("the addressed runner did not receive its cancellation".into());
    };
    assert_eq!(message.subject, addressed);
    assert_eq!(serde_json::from_str::<Command>(&message.payload)?, command);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), other.recv())
            .await
            .is_err(),
        "a different runner stays silent for one second"
    );
    drop(recipient);
    drop(other);
    publishing.shutdown().await?;
    receiving.shutdown().await?;
    Ok(())
}

/// One bus on the first node and one on the third, sharing a namespace.
async fn replicas() -> Fallible<(afd_bus::Handle, afd_bus::Handle)> {
    let seeds = seeds()?;
    let ns = namespace();
    let first = seeds.first().ok_or("node 1")?;
    let third = seeds.get(2).ok_or("node 3")?;
    Ok((
        bus(first, &ns, Tuning::default()).await?,
        bus(third, &ns, Tuning::default()).await?,
    ))
}

/// Ring every fleet until both replicas hold the whole set. A ring is a hint,
/// so ringing again before interest reaches the publisher's node is harmless.
async fn receive(
    publisher: &afd_bus::Handle,
    wakes: &[Wake; 2],
    expected: &BTreeSet<Uuid7>,
) -> TestResult {
    let mut received = [BTreeSet::new(), BTreeSet::new()];
    tokio::time::timeout(PATIENCE, async {
        loop {
            for fleet in expected {
                wake::publish(publisher, fleet.clone()).await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            for (wake, seen) in wakes.iter().zip(&mut received) {
                while let Some(fleet) = wake.take() {
                    assert!(
                        expected.contains(&fleet),
                        "a malformed ring fabricates no fleet"
                    );
                    seen.insert(fleet);
                }
            }
            if received.iter().all(|seen| seen == expected) {
                return;
            }
        }
    })
    .await?;
    Ok(())
}
