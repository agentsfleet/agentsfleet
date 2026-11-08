//! The tail's opening, its announced gaps and its retry pacing.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::Subject;
use futures_util::StreamExt as _;

use super::Resubscribe;
use crate::ceiling::Ceiling;
use crate::live::Live;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const FLEET: &str = "0195b4ba-8d3a-7000-8abc-000000000001";

fn kind(frame: &crate::frame::Frame) -> String {
    frame.kind.to_string()
}

#[test]
fn retries_double_to_their_ceiling() {
    let policy = Resubscribe::default();
    assert_eq!(policy.wait(0), Duration::from_millis(250));
    assert_eq!(policy.wait(1), Duration::from_millis(500));
    assert_eq!(policy.wait(4), Duration::from_secs(4));
    assert_eq!(policy.wait(5), Duration::from_secs(5), "capped");
    assert_eq!(
        policy.wait(u32::MAX),
        Duration::from_secs(5),
        "never overflows"
    );
}

#[tokio::test]
async fn tail_sends_catching_up_once_after_subscribing() -> TestResult {
    let (bus, server) = afd_bus::test_util::detached()?;
    let fleet = Uuid7::parse(FLEET)?;
    let live = Live::new(bus, Ceiling::new(4));
    let mut tail = live.tail_of(&fleet).await;
    let hello = tail.next().await.ok_or("hello")?;
    let caught_up = tail.next().await.ok_or("catching_up")?;
    assert_eq!(
        (kind(&hello), kind(&caught_up)),
        ("hello".to_owned(), "catching_up".to_owned())
    );
    assert!(
        caught_up.data.to_string().contains(r#""dropped":0"#),
        "{}",
        caught_up.data
    );
    server.publish(&Subject::FleetTail(fleet), r#"{"kind":"event_admitted"}"#);
    let admitted = tail.next().await.ok_or("the published frame")?;
    assert!(
        admitted.data.to_string().contains("event_admitted"),
        "{}",
        admitted.data
    );
    Ok(())
}

#[tokio::test]
async fn tail_subscribe_failure_sends_catching_up() -> TestResult {
    let live = Live::detached(Ceiling::new(4));
    let mut tail = live.tail_of(&Uuid7::parse(FLEET)?).await;
    let hello = tokio::time::timeout(Duration::from_secs(1), tail.next())
        .await?
        .ok_or("hello")?;
    let caught_up = tokio::time::timeout(Duration::from_secs(1), tail.next())
        .await?
        .ok_or("catching_up")?;
    assert_eq!(kind(&hello), "hello");
    assert_eq!(kind(&caught_up), "catching_up", "never only heartbeats");
    Ok(())
}

#[tokio::test]
async fn a_closed_bus_ends_the_tail_so_the_client_reconnects_elsewhere() -> TestResult {
    let (bus, _server) = afd_bus::test_util::detached()?;
    let live = Live::new(bus.clone(), Ceiling::new(4));
    let mut tail = live.tail_of(&Uuid7::parse(FLEET)?).await;
    for _ in 0..2 {
        tail.next().await.ok_or("the opening")?;
    }
    bus.shutdown().await?;
    let ended = tokio::time::timeout(Duration::from_secs(1), tail.next()).await?;
    assert!(ended.is_none(), "a draining bus ends the stream: {ended:?}");
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_failed_subscribe_is_announced_once_then_retried_quietly() -> TestResult {
    let live = Live::detached(Ceiling::new(4));
    let mut tail = live.tail_of(&Uuid7::parse(FLEET)?).await;
    for _ in 0..2 {
        tail.next().await.ok_or("the opening")?;
    }
    let quiet = tokio::time::timeout(Duration::from_secs(60), tail.next()).await;
    assert!(quiet.is_err(), "retries stay quiet: {quiet:?}");
    Ok(())
}
