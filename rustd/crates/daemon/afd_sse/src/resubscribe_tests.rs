//! The tail's opening, its announced gaps and its retry pacing.

use std::time::Duration;

use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::id::Uuid7;
use afd_bus::{Bus, Handle, Subject, Subscription};
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

#[tokio::test(start_paused = true)]
async fn a_tail_opened_on_a_closed_bus_ends_rather_than_retrying() -> TestResult {
    let (bus, _server) = afd_bus::test_util::detached()?;
    bus.shutdown().await?;
    let live = Live::new(bus, Ceiling::new(4));
    let mut tail = live.tail_of(&Uuid7::parse(FLEET)?).await;
    for _ in 0..2 {
        tail.next().await.ok_or("the opening")?;
    }
    let ended = tokio::time::timeout(Duration::from_secs(60), tail.next()).await?;
    assert!(ended.is_none(), "a closed bus is never retried: {ended:?}");
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

/// A bus that refuses its first `refusals` subscribes, then serves from `live`.
#[derive(Debug)]
struct Flaky {
    refusals: AtomicUsize,
    live: Handle,
}

#[async_trait::async_trait]
impl Bus for Flaky {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        self.live.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> afd_bus::Result<()> {
        self.live.publish_batch(subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> afd_bus::Result<Subscription> {
        let refusing = self
            .refusals
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if refusing {
            // A transient refusal, as a timed-out subscribe is: the tail retries.
            match tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await {
                Err(elapsed) => Err(elapsed.into()),
                Ok(()) => self.live.subscribe(subject).await,
            }
        } else {
            self.live.subscribe(subject).await
        }
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.live.shutdown().await
    }
}

#[tokio::test]
async fn a_tail_that_failed_to_subscribe_recovers_with_catching_up() -> TestResult {
    let (live_bus, server) = afd_bus::test_util::detached()?;
    let flaky = Handle::new(Flaky {
        refusals: AtomicUsize::new(3),
        live: live_bus,
    });
    let fleet = Uuid7::parse(FLEET)?;
    let fast = Resubscribe {
        first: Duration::from_millis(1),
        ceiling: Duration::from_millis(4),
    };
    let live = Live::new(flaky, Ceiling::new(4)).with_resubscribe(fast);
    let mut tail = live.tail_of(&fleet).await;
    for expected in ["hello", "catching_up"] {
        let frame = tail.next().await.ok_or("the opening")?;
        assert_eq!(kind(&frame), expected);
    }

    let recovered = tokio::time::timeout(Duration::from_secs(1), tail.next())
        .await?
        .ok_or("the recovery")?;
    assert_eq!(
        kind(&recovered),
        "catching_up",
        "the viewer backfills the outage"
    );
    server.publish(&Subject::FleetTail(fleet), r#"{"kind":"event_admitted"}"#);
    let admitted = tail.next().await.ok_or("the published frame")?;
    assert!(admitted.data.to_string().contains("event_admitted"));
    Ok(())
}

#[tokio::test]
async fn a_gap_in_the_tail_is_announced() -> TestResult {
    let (bus, server) = afd_bus::test_util::detached()?;
    let fleet = Uuid7::parse(FLEET)?;
    let live = Live::new(bus, Ceiling::new(4));
    let mut tail = live.tail_of(&fleet).await;
    for _ in 0..2 {
        tail.next().await.ok_or("the opening")?;
    }
    // The first confirmation is the subscription's own; a repeat is a gap.
    server.confirm(&Subject::FleetTail(fleet.clone()));
    server.confirm(&Subject::FleetTail(fleet));
    let gap = tokio::time::timeout(Duration::from_secs(1), tail.next())
        .await?
        .ok_or("the gap")?;
    assert_eq!(kind(&gap), "catching_up");
    Ok(())
}

#[tokio::test]
async fn a_lagging_viewer_is_told_how_much_it_missed() -> TestResult {
    let (bus, server) = afd_bus::test_util::detached()?;
    let fleet = Uuid7::parse(FLEET)?;
    let live = Live::new(bus, Ceiling::new(4));
    let mut tail = live.tail_of(&fleet).await;
    for _ in 0..2 {
        tail.next().await.ok_or("the opening")?;
    }
    // Far past any reader's buffer, published before the viewer reads.
    for n in 0..20_000 {
        server.publish(
            &Subject::FleetTail(fleet.clone()),
            &format!(r#"{{"n":{n}}}"#),
        );
    }
    let mut lagged = None;
    while lagged.is_none() {
        let frame = tokio::time::timeout(Duration::from_secs(1), tail.next())
            .await?
            .ok_or("the tail")?;
        if kind(&frame) == "catching_up" {
            lagged = Some(frame);
        }
    }
    let lagged = lagged.ok_or("a lag frame")?;
    assert!(
        !lagged.data.to_string().contains(r#""dropped":0"#),
        "a lag names what it dropped: {}",
        lagged.data
    );
    Ok(())
}
