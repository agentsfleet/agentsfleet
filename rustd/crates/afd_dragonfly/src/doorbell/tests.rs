use super::Doorbell;
use afd_core::id::Uuid7;
use afd_observability::test_util::Capture;

fn fleet(index: usize) -> afd_core::error::Result<Uuid7> {
    Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}"))
}

#[test]
fn duplicate_hints_are_shared_and_consumed_once() -> afd_core::error::Result<()> {
    let bell = Doorbell::new();
    let reader = bell.clone();
    let fleet = fleet(1)?;
    bell.ring(fleet.clone());
    bell.ring(fleet.clone());
    assert_eq!(reader.take(), Some(fleet));
    assert_eq!(bell.take(), None);
    Ok(())
}

#[test]
fn a_burst_discards_old_hints_without_growing_the_buffer() -> afd_core::error::Result<()> {
    let bell = Doorbell::new();
    for index in 0..100 {
        bell.ring(fleet(index)?);
    }
    let hints: Vec<_> = std::iter::from_fn(|| bell.take()).collect();
    assert_eq!(hints.len(), 64);
    assert_eq!(hints.first(), Some(&fleet(36)?));
    assert_eq!(hints.last(), Some(&fleet(99)?));
    Ok(())
}

#[test]
fn concurrent_duplicate_notifications_cannot_duplicate_a_claim_hint() -> afd_core::error::Result<()>
{
    let bell = Doorbell::new();
    let fleet = fleet(1)?;
    std::thread::scope(|scope| {
        for _ in 0..100 {
            let bell = bell.clone();
            let fleet = fleet.clone();
            scope.spawn(move || bell.ring(fleet));
        }
    });
    assert_eq!(bell.take(), Some(fleet));
    assert_eq!(bell.take(), None);
    Ok(())
}

#[tokio::test]
async fn shutdown_releases_every_doorbell_listener_and_its_buffer() {
    let (hub, _server) = crate::SubscriptionHub::detached();
    let bell = Doorbell::new();
    let buffer = std::sync::Arc::downgrade(&bell.hints);
    bell.listen(&hub);
    for shard in 0..super::WAKE_SHARDS {
        assert_eq!(hub.readers(&super::channel(shard)), 1);
    }
    drop(bell);
    hub.shutdown();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while buffer.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(stopped.is_ok(), "hub shutdown must release every listener");
}

#[tokio::test]
async fn gaps_and_lag_are_counted_without_losing_the_next_valid_hint() -> afd_core::error::Result<()>
{
    let metrics = Capture::install();
    let family = "agentsfleet_fleet_ready_wake_failures_total";
    let labels = [("kind", "gap")];
    let before = metrics.sum(family, &labels);
    let (hub, server) = crate::SubscriptionHub::detached();
    let channel = super::channel(0);
    let subscription = hub.subscribe(&channel);
    let bell = Doorbell::new();
    server.confirm(&channel);
    server.confirm(&channel);
    // Overflow the broadcast buffer before its listener starts. This loses
    // the first gap; the later confirmation supplies a separate reconnect gap.
    for _ in 0..=crate::hub::CHANNEL_CAPACITY {
        server.publish(&channel, "invalid fleet");
    }
    server.confirm(&channel);
    let expected = fleet(1)?;
    server.publish(&channel, expected.as_str());
    hub.shutdown();
    let stopped = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        super::answer(subscription, bell.clone()),
    )
    .await;
    assert!(
        stopped.is_ok(),
        "the listener drains and exits after shutdown"
    );
    assert_eq!(bell.take(), Some(expected));
    assert_eq!(bell.take(), None, "invalid and lost frames create no hints");
    assert_eq!(metrics.sum(family, &labels) - before, 2);
    assert_eq!(
        hub.readers(&channel),
        0,
        "the listener releases its channel"
    );
    Ok(())
}
