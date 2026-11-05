use super::Doorbell;
use afd_core::id::Uuid7;
use afd_observability::test_util::Capture;

fn fleet(index: usize) -> afd_core::error::Result<Uuid7> {
    Uuid7::parse(&format!("0195b4ba-8d3a-7000-8abc-{index:012x}"))
}

#[tokio::test]
async fn shutdown_releases_every_doorbell_listener() {
    let (hub, _server) = crate::SubscriptionHub::detached();
    let bell = Doorbell::new();
    let buffer = std::sync::Arc::downgrade(&bell.rings);
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
async fn gaps_and_lag_are_counted_without_losing_the_next_valid_ring() -> afd_core::error::Result<()>
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
    server.publish(&channel, fleet(1)?.as_str());
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
    // The one valid frame left one permit; the invalid and lost frames left
    // none, so a second wait finds nothing and runs to its deadline.
    let first = bell.wait_for_ring(SHORT, 1).await;
    let second = bell.wait_for_ring(SHORT, 1).await;
    assert_eq!(
        (first, second),
        (super::Waited::Rung, super::Waited::Deadline),
        "one valid frame rings once; invalid and lost frames ring nothing"
    );
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

/// Long enough for a woken wait to have answered, short enough to wait out.
const SHORT: std::time::Duration = std::time::Duration::from_millis(50);

/// A held wait ends on the next ring, and gives its place back when it does.
#[tokio::test]
async fn a_ring_answers_a_held_wait() {
    let bell = Doorbell::new();
    let (waited, ()) = tokio::join!(bell.wait_for_ring(LONG, 2), async {
        tokio::task::yield_now().await;
        bell.ring();
    });
    assert_eq!((waited, bell.held()), (super::Waited::Rung, 0));
}

/// A ring with no beat held is kept: the next beat answers at once, and only
/// that one.
#[tokio::test]
async fn a_ring_before_the_hold_is_kept_for_the_next_beat() {
    let bell = Doorbell::new();
    bell.ring();
    let first = bell.wait_for_ring(LONG, 1).await;
    let second = bell.wait_for_ring(SHORT, 1).await;
    assert_eq!(
        (first, second),
        (super::Waited::Rung, super::Waited::Deadline)
    );
}

/// Waits until `count` beats are held on `bell`.
async fn until_held(bell: &Doorbell, count: usize) {
    while bell.held() < count {
        tokio::task::yield_now().await;
    }
}

/// With three beats held, one ring completes exactly one; the
/// other two are still held after it has answered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_a_ring_completes_exactly_one_held_beat() {
    let bell = Doorbell::new();
    let mut waits: Vec<_> = (0..3)
        .map(|_| {
            let bell = bell.clone();
            tokio::spawn(async move { bell.wait_for_ring(LONG, 3).await })
        })
        .collect();
    until_held(&bell, 3).await;
    bell.ring();
    let (rung, _index, rest) = futures_util::future::select_all(waits.drain(..)).await;
    tokio::time::sleep(SHORT).await;
    assert_eq!(
        (
            rung.ok(),
            bell.held(),
            rest.iter().filter(|wait| wait.is_finished()).count()
        ),
        (Some(super::Waited::Rung), 2, 0),
        "one ring answers one beat and leaves the other two held"
    );
    bell.close();
}

/// Closing ends every held beat at once, and a wait after the
/// close is not held at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_close_ends_every_held_beat() {
    let bell = Doorbell::new();
    let waits: Vec<_> = (0..3)
        .map(|_| {
            let bell = bell.clone();
            tokio::spawn(async move { bell.wait_for_ring(LONG, 3).await })
        })
        .collect();
    until_held(&bell, 3).await;
    bell.close();
    let mut answers = Vec::new();
    for wait in waits {
        answers.push(
            tokio::time::timeout(SHORT * 20, wait)
                .await
                .ok()
                .and_then(Result::ok),
        );
    }
    let after = tokio::time::timeout(SHORT, bell.wait_for_ring(LONG, 3)).await;
    assert_eq!(
        (answers, after.ok(), bell.held()),
        (
            vec![Some(super::Waited::Deadline); 3],
            Some(super::Waited::Deadline),
            0
        )
    );
}

/// Past the cap a beat answers `Capped` at once, and a held
/// beat dropped mid-wait gives its place back to a new one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_held_beats_past_the_cap_answer_capped() {
    const CAP: usize = 64;
    let bell = Doorbell::new();
    let waits: Vec<_> = (0..CAP)
        .map(|_| {
            let bell = bell.clone();
            tokio::spawn(async move { bell.wait_for_ring(LONG, CAP).await })
        })
        .collect();
    until_held(&bell, CAP).await;
    let past_the_cap = bell.wait_for_ring(LONG, CAP).await;
    let mut waits = waits.into_iter();
    if let Some(dropped) = waits.next() {
        dropped.abort();
        let _cancelled = dropped.await;
    }
    let replacement = {
        let bell = bell.clone();
        tokio::spawn(async move { bell.wait_for_ring(LONG, CAP).await })
    };
    until_held(&bell, CAP).await;
    assert_eq!(
        (past_the_cap, replacement.is_finished()),
        (super::Waited::Capped, false),
        "the 65th beat is refused, and a dropped beat's place is held again"
    );
    bell.close();
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
async fn a_wait_past_the_cap_is_refused_at_once() {
    let bell = Doorbell::new();
    let (held, refused) = tokio::join!(bell.wait_for_ring(LONG, 1), async {
        let refused = bell.wait_for_ring(LONG, 1).await;
        let while_held = bell.held();
        bell.ring();
        (refused, while_held)
    });
    assert_eq!(
        (held, refused, bell.held()),
        (super::Waited::Rung, (super::Waited::Capped, 1), 0)
    );
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
/// and thirty-six refused, one ring per held wait answers them all, and every
/// place is given back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_cap_holds_under_contention() {
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
    for _ in 0..CAP {
        bell.ring();
    }
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
}
