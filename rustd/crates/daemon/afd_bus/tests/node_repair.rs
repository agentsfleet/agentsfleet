//! A real hub repairs a cut TCP node and exposes its gap through the bus.
#![cfg(feature = "test-util")]

use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::{Received, Subject, Subscription, test_util};
use afd_dragonfly::{
    DragonflyConfig, SubscriptionHub,
    config::DragonflyRole,
    test_util::fake_redis::{FakeRedis, Reply},
};

type TestResult<T = ()> = afd_bus::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = afd_dragonfly::test_util::NODE_REPAIR_WINDOW.saturating_mul(2);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_repaired_node_gaps_every_bus_subscription() -> TestResult {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("SSUBSCRIBE", Reply::SubscribeAck),
        ("SUNSUBSCRIBE", Reply::UnsubscribeAck),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url());
    let hub = SubscriptionHub::start(config).await?;
    let bus = test_util::with_hub(hub.clone())?;
    let first_subject =
        Subject::RunnerCommand(Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000d001")?);
    let second_subject =
        Subject::RunnerCommand(Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000d002")?);
    let mut first = bus.subscribe(&first_subject).await?;
    let mut second = bus.subscribe(&second_subject).await?;
    deliver(&server, &first_subject, "before-first-cut", &mut first).await?;
    deliver(&server, &second_subject, "before-second-cut", &mut second).await?;
    let subscribed = subscribes_seen(&server);
    server.cut();
    gap_on(&mut first).await?;
    gap_on(&mut second).await?;
    deliver(&server, &first_subject, "after-first-repair", &mut first).await?;
    deliver(&server, &second_subject, "after-second-repair", &mut second).await?;
    assert_eq!(hub.connections_opened(), 1, "repair retains the native hub");
    assert!(subscribes_seen(&server) >= subscribed + 2);
    drop(first);
    drop(second);
    bus.shutdown().await?;
    tokio::time::timeout(BOUND, async {
        while server.live_connections() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

async fn deliver(
    server: &FakeRedis,
    subject: &Subject,
    payload: &str,
    reader: &mut Subscription,
) -> TestResult {
    let channel = test_util::physical_channel(subject);
    tokio::time::timeout(BOUND, async {
        loop {
            server.publish(&channel, payload);
            match tokio::time::timeout(Duration::from_millis(10), reader.recv()).await {
                Ok(Ok(Received::Message(message))) if message.payload.as_str() == payload => {
                    assert_eq!(&message.subject, subject);
                    return TestResult::Ok(());
                }
                Ok(Ok(Received::Message(_) | Received::Gap | Received::Lagged(_))) | Err(_) => {}
                Ok(Err(error)) => return Err(error.into()),
            }
        }
    })
    .await??;
    Ok(())
}

async fn gap_on(reader: &mut Subscription) -> TestResult {
    tokio::time::timeout(BOUND, async {
        loop {
            match reader.recv().await? {
                Received::Gap => return TestResult::Ok(()),
                Received::Message(_) | Received::Lagged(_) => {}
            }
        }
    })
    .await??;
    Ok(())
}

fn subscribes_seen(server: &FakeRedis) -> usize {
    server
        .seen()
        .iter()
        .filter(|name| *name == "SSUBSCRIBE")
        .count()
}
