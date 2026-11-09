//! The NATS adapter against the lane's three-node cluster.

mod support;

#[path = "nats_exclusive/mod.rs"]
mod integration_bus_exclusive;

use std::{sync::Arc, time::Duration};

use af_core::{error_code, id::Uuid7};
use afd_bus::{Received, Subject, nats::Tuning};

use support::{
    Fallible, TestResult, bus, namespace, next, port_of, proxy::Proxy, seeds, settle, until, via,
};

const LIVE: &str = "needs live NATS: make test-integration-rustd";

fn fleet(n: u8) -> Fallible<Subject> {
    Ok(Subject::FleetTail(Uuid7::parse(&format!(
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb0{n:02}"
    ))?))
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn a_message_crosses_nodes_to_every_reader_once() -> TestResult {
    let _ = LIVE;
    let seeds = seeds()?;
    let ns = namespace();
    let first = seeds.first().ok_or("node 1")?;
    let third = seeds.get(2).ok_or("node 3")?;
    let publisher = bus(first, &ns, Tuning::default()).await?;
    let subscriber = bus(third, &ns, Tuning::default()).await?;
    let subject = fleet(1)?;
    let mut one = subscriber.subscribe(&subject).await?;
    let mut two = subscriber.subscribe(&subject).await?;
    settle(&publisher, &subject, &mut [&mut one, &mut two]).await?;
    publisher
        .publish_batch(&subject, &["a".to_owned(), "b".to_owned()])
        .await?;
    let mut payloads = Vec::new();
    for subscription in [&mut one, &mut two] {
        for _ in 0..2 {
            let Received::Message(message) = next(subscription).await? else {
                return Err("only messages arrive on a healthy cluster".into());
            };
            payloads.push(message);
        }
    }
    let texts: Vec<&str> = payloads.iter().map(|m| m.payload.as_str()).collect();
    assert_eq!(texts, ["a", "b", "a", "b"], "in order, to each reader");
    let (Some(a1), Some(a2)) = (payloads.first(), payloads.get(2)) else {
        return Err("four payloads".into());
    };
    assert!(
        Arc::ptr_eq(&a1.payload, &a2.payload),
        "one payload per message, shared"
    );
    assert_eq!(a1.subject, subject);
    publisher.shutdown().await?;
    subscriber.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn reconnect_gaps_every_subscription() -> TestResult {
    let metrics = af_observability::test_util::Capture::install();
    let reconnect_gaps = || metrics.sum("agentsfleet_bus_gaps_total", &[("cause", "reconnect")]);
    let seeds = seeds()?;
    let ns = namespace();
    let first = seeds.first().ok_or("node 1")?;
    let proxy = Proxy::start(port_of(first)?).await?;
    let subscriber = bus(&via(first, proxy.port())?, &ns, Tuning::default()).await?;
    let publisher = bus(seeds.get(1).ok_or("node 2")?, &ns, Tuning::default()).await?;
    let subjects = [fleet(1)?, fleet(2)?, fleet(3)?];
    let mut subscriptions = Vec::new();
    for subject in &subjects {
        let mut subscription = subscriber.subscribe(subject).await?;
        settle(&publisher, subject, &mut [&mut subscription]).await?;
        subscriptions.push(subscription);
    }
    let counted = reconnect_gaps();
    proxy.cut().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    proxy.restore();
    for (subject, subscription) in subjects.iter().zip(&mut subscriptions) {
        assert_eq!(
            next(subscription).await?,
            Received::Gap,
            "a gap comes first"
        );
        let before = until(&publisher, subject, subscription, "after").await?;
        assert!(
            !before.contains(&Received::Gap),
            "exactly one gap: {before:?}"
        );
    }
    assert_eq!(
        reconnect_gaps() - counted,
        3,
        "one gap counted per subscription told"
    );
    publisher.shutdown().await?;
    subscriber.shutdown().await?;
    Ok(())
}

#[test]
#[ignore = "needs live NATS: make test-integration-rustd"]
fn slow_consumer_gaps_every_subscription_on_the_client() -> TestResult {
    let metrics = af_observability::test_util::Capture::install();
    let slow_gaps = || metrics.sum("agentsfleet_bus_gaps_total", &[("cause", "slow_consumer")]);
    let counted = slow_gaps();
    let seeds = seeds()?;
    let ns = namespace();
    let node = seeds.first().ok_or("node 1")?.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let small = Tuning {
        subscription_capacity: 8,
        ..Tuning::default()
    };
    let subscriber = runtime.block_on(bus(&node, &ns, small))?;
    let subject = fleet(1)?;
    let other = fleet(2)?;
    let (mut held, mut quiet) = runtime.block_on(async {
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((
            subscriber.subscribe(&subject).await?,
            subscriber.subscribe(&other).await?,
        ))
    })?;
    // The publisher runs on its own thread and runtime, so its frames reach the
    // server while this runtime, and with it the subscriber's pump, is held.
    let (sent, finished) = std::sync::mpsc::channel();
    let publisher_ns = ns;
    let publisher_subject = subject;
    let worker = std::thread::spawn(move || -> TestResult {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let publisher = bus(&node, &publisher_ns, Tuning::default()).await?;
            for index in 0..100 {
                publisher
                    .publish(&publisher_subject, &index.to_string())
                    .await?;
            }
            publisher.shutdown().await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })?;
        sent.send(())?;
        Ok(())
    });
    finished.recv_timeout(support::PATIENCE)?;
    worker
        .join()
        .map_err(|panic| format!("publisher panicked: {panic:?}"))??;
    runtime.block_on(async {
        let mut saw_gap = false;
        while let Ok(Ok(received)) = tokio::time::timeout(Duration::from_secs(2), held.recv()).await
        {
            if received == Received::Gap {
                saw_gap = true;
                break;
            }
        }
        assert!(saw_gap, "the held subscription is told it lost frames");
        assert_eq!(next(&mut quiet).await?, Received::Gap, "so is every other");
        assert!(
            slow_gaps() - counted >= 2,
            "each subscription on the client counts as told"
        );
        subscriber.shutdown().await?;
        Ok(())
    })
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn failed_publish_sends_a_gap_marker_on_reconnect() -> TestResult {
    let seeds = seeds()?;
    let ns = namespace();
    let first = seeds.first().ok_or("node 1")?;
    let proxy = Proxy::start(port_of(first)?).await?;
    let publisher = bus(&via(first, proxy.port())?, &ns, Tuning::default()).await?;
    let subscriber = bus(first, &ns, Tuning::default()).await?;
    let subject = fleet(1)?;
    let mut subscription = subscriber.subscribe(&subject).await?;
    settle(&publisher, &subject, &mut [&mut subscription]).await?;
    publisher.publish(&subject, "before").await?;
    proxy.cut().await;
    let _ = publisher.publish(&subject, "lost").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    proxy.restore();
    let before = until(&publisher, &subject, &mut subscription, "after").await?;
    assert!(
        before.contains(&Received::Gap),
        "the subscriber is told before the next frame: {before:?}"
    );
    publisher.shutdown().await?;
    subscriber.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn an_overflowing_debt_gaps_every_hub_through_the_broadcast() -> TestResult {
    let metrics = af_observability::test_util::Capture::install();
    let debt_gaps = || metrics.sum("agentsfleet_bus_gaps_total", &[("cause", "publish_debt")]);
    let counted = debt_gaps();
    let seeds = seeds()?;
    let ns = namespace();
    let first = seeds.first().ok_or("node 1")?;
    let proxy = Proxy::start(port_of(first)?).await?;
    let tiny = Tuning {
        debt_subjects_max: 1,
        ..Tuning::default()
    };
    let publisher = bus(&via(first, proxy.port())?, &ns, tiny).await?;
    let subscriber = bus(seeds.get(2).ok_or("node 3")?, &ns, Tuning::default()).await?;
    let unrelated = fleet(9)?;
    let mut subscription = subscriber.subscribe(&unrelated).await?;
    settle(&publisher, &unrelated, &mut [&mut subscription]).await?;
    publisher.publish(&fleet(1)?, "a").await?;
    publisher.publish(&fleet(2)?, "b").await?;
    proxy.cut().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    proxy.restore();
    let before = until(&publisher, &unrelated, &mut subscription, "after").await?;
    assert!(
        before.contains(&Received::Gap),
        "a subject the publisher never named still gaps: {before:?}"
    );
    assert!(
        debt_gaps() - counted >= 1,
        "the subscriber's hub counts the broadcast gap"
    );
    publisher.shutdown().await?;
    subscriber.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn a_payload_that_is_not_utf8_never_reaches_a_reader() -> TestResult {
    let seeds = seeds()?;
    let ns = namespace();
    let node = seeds.first().ok_or("node 1")?;
    let subscriber = bus(node, &ns, Tuning::default()).await?;
    let subject = fleet(1)?;
    let mut subscription = subscriber.subscribe(&subject).await?;
    settle(&subscriber, &subject, &mut [&mut subscription]).await?;
    let raw = async_nats::ConnectOptions::with_user_and_password(
        "agentsfleet".to_owned(),
        "agentsfleet".to_owned(),
    )
    .connect(port_and_host(node)?)
    .await?;
    let physical = format!("dev.{ns}.af.fleet.0195b4ba-8d3a-7f13-8abc-2b3e1e0bb001.tail");
    raw.publish(physical.clone(), bytes::Bytes::from_static(b"\xff\xfe"))
        .await?;
    raw.publish(physical, bytes::Bytes::from_static(b"valid"))
        .await?;
    raw.flush().await?;
    let Received::Message(message) = next(&mut subscription).await? else {
        return Err("only the valid payload arrives".into());
    };
    assert_eq!(message.payload.as_str(), "valid");
    subscriber.shutdown().await?;
    Ok(())
}

fn port_and_host(seed: &str) -> Fallible<String> {
    Ok(format!("127.0.0.1:{}", port_of(seed)?))
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn shutdown_closes_everything_once() -> TestResult {
    let seeds = seeds()?;
    let bus = bus(
        seeds.first().ok_or("node 1")?,
        &namespace(),
        Tuning::default(),
    )
    .await?;
    let subject = fleet(1)?;
    let mut open = bus.subscribe(&subject).await?;
    bus.shutdown().await?;
    bus.shutdown().await?;
    let Err(error) = bus.publish(&subject, "late").await else {
        return Err("a closed bus refuses publishes".into());
    };
    assert_eq!(error.reason(), "closed");
    let Err(error) = bus.subscribe(&subject).await else {
        return Err("a closed bus refuses subscriptions".into());
    };
    assert_eq!(error.reason(), "closed");
    let Err(_) = next(&mut open).await else {
        return Err("an open subscription ends".into());
    };
    Ok(())
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn boot_refuses_an_unreachable_seed() -> TestResult {
    let quick = Tuning {
        connect_timeout_ms: 200,
        ..Tuning::default()
    };
    let Err(error) = bus(
        "nats://agentsfleet:agentsfleet@127.0.0.1:1",
        &namespace(),
        quick,
    )
    .await
    else {
        return Err("a dead seed must refuse boot".into());
    };
    let error = error
        .downcast::<afd_bus::Error>()
        .map_err(|other| format!("not a bus error: {other}"))?;
    assert_eq!(error.code(), error_code::STARTUP_DRAGONFLY_CONNECT);
    assert!(error.to_string().contains("NATS_URL"), "{error}");
    Ok(())
}
