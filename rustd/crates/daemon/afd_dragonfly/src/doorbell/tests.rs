use super::Doorbell;
use af_core::id::Uuid7;
use af_observability::test_util::Capture;

fn fleet(index: usize) -> af_core::error::Result<Uuid7> {
    Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}"))
}

#[test]
fn duplicate_hints_are_shared_and_consumed_once() -> af_core::error::Result<()> {
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
fn a_burst_discards_old_hints_without_growing_the_buffer() -> af_core::error::Result<()> {
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
fn concurrent_duplicate_notifications_cannot_duplicate_a_claim_hint() -> af_core::error::Result<()>
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
async fn gaps_and_lag_are_counted_without_losing_the_next_valid_hint() -> af_core::error::Result<()>
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

/// Longer than any test waits, so a wait that ends early ended for a reason.
const LONG: std::time::Duration = std::time::Duration::from_secs(30);

/// A held wait ends on the next ring, and gives its place back when it does.
#[tokio::test]
async fn a_ring_answers_a_held_wait() -> af_core::error::Result<()> {
    let bell = Doorbell::new();
    let fleet = fleet(1)?;
    let (waited, ()) = tokio::join!(bell.wait_for_ring(LONG, 2), async {
        tokio::task::yield_now().await;
        bell.ring(fleet);
    });
    assert_eq!((waited, bell.held()), (super::Waited::Rung, 0));
    Ok(())
}

/// With no ring the wait ends at its deadline, and gives its place back.
#[tokio::test]
async fn a_silent_wait_answers_at_its_deadline() {
    let bell = Doorbell::new();
    let waited = bell
        .wait_for_ring(std::time::Duration::from_millis(20), 1)
        .await;
    assert_eq!((waited, bell.held()), (super::Waited::Deadline, 0));
}

/// A wait past the cap is refused at once while the held one keeps waiting.
#[tokio::test]
async fn a_wait_past_the_cap_is_refused_at_once() -> af_core::error::Result<()> {
    let bell = Doorbell::new();
    let fleet = fleet(1)?;
    let (held, refused) = tokio::join!(bell.wait_for_ring(LONG, 1), async {
        let refused = bell.wait_for_ring(LONG, 1).await;
        let while_held = bell.held();
        bell.ring(fleet);
        (refused, while_held)
    });
    assert_eq!(
        (held, refused, bell.held()),
        (super::Waited::Rung, (super::Waited::Capped, 1), 0)
    );
    Ok(())
}

/// A second admission for a fleet whose hint is still buffered is fresh work
/// too: the generation moves before the hint dedup, so the wait hears it.
#[tokio::test]
async fn a_ring_for_a_buffered_fleet_still_wakes_a_wait() -> af_core::error::Result<()> {
    let bell = Doorbell::new();
    let fleet = fleet(1)?;
    bell.ring(fleet.clone());
    let (waited, ()) = tokio::join!(bell.wait_for_ring(LONG, 1), async {
        tokio::task::yield_now().await;
        bell.ring(fleet.clone());
    });
    assert_eq!(
        (waited, bell.take(), bell.take()),
        (super::Waited::Rung, Some(fleet), None),
        "woken, with the hint still buffered once"
    );
    Ok(())
}

/// Closing ends a held wait at once, as at its deadline, and a wait after the
/// close is not held at all.
#[tokio::test]
async fn a_closed_doorbell_ends_and_refuses_held_waits() {
    let bell = Doorbell::new();
    let (held, ()) = tokio::join!(bell.wait_for_ring(LONG, 1), async {
        tokio::task::yield_now().await;
        bell.close();
    });
    let after = tokio::time::timeout(
        std::time::Duration::from_millis(10),
        bell.wait_for_ring(LONG, 1),
    )
    .await;
    assert_eq!(
        (held, after, bell.held()),
        (super::Waited::Deadline, Ok(super::Waited::Deadline), 0)
    );
}

/// A hundred waits racing for sixty-four places: exactly sixty-four are held
/// and thirty-six refused, one ring answers every held one, and every place
/// is given back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_cap_holds_under_contention() -> af_core::error::Result<()> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    const WAITS: usize = 100;
    const CAP: usize = 64;
    let bell = Doorbell::new();
    let refused = Arc::new(AtomicUsize::new(0));
    let waits: Vec<_> = (0..WAITS)
        .map(|_| {
            let bell = bell.clone();
            let refused = Arc::clone(&refused);
            tokio::spawn(async move {
                let waited = bell.wait_for_ring(LONG, CAP).await;
                if waited == super::Waited::Capped {
                    refused.fetch_add(1, Ordering::AcqRel);
                }
                waited
            })
        })
        .collect();
    // Ring only once every wait is either held or refused, so no late wait
    // takes a place a rung one gave back.
    while bell.held() < CAP || refused.load(Ordering::Acquire) < WAITS - CAP {
        tokio::task::yield_now().await;
    }
    bell.ring(fleet(1)?);
    let mut answers = Vec::new();
    for wait in waits {
        // A wait that panicked counts as neither, and fails the totals below.
        answers.push(wait.await.unwrap_or(super::Waited::Deadline));
    }
    let count = |of: super::Waited| answers.iter().filter(|answer| **answer == of).count();
    assert_eq!(
        (
            count(super::Waited::Rung),
            count(super::Waited::Capped),
            bell.held()
        ),
        (CAP, WAITS - CAP, 0)
    );
    Ok(())
}
