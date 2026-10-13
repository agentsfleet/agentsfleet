//! The doorbell's bits: what a ring sets, what a take clears, and that two
//! takes racing for one bit cannot both win it.

use std::sync::{Arc, Barrier};

use super::{Doorbell, first_at_or_after};
use crate::ready::partition::{Partition, READY_PARTITIONS};

fn partition(index: u16) -> Partition {
    Partition::new(index).unwrap_or_else(|| unreachable!("{index} is below {READY_PARTITIONS}"))
}

/// A ring is taken once: the take clears the bit it returns.
#[test]
fn test_a_ring_is_taken_once() {
    let doorbell = Doorbell::new();
    doorbell.ring(partition(5));

    assert_eq!(doorbell.take(partition(0)), Some(partition(5)));
    assert_eq!(doorbell.take(partition(0)), None, "the take cleared it");
}

/// A take starts at the poll's cursor and wraps, so polls at different
/// cursors are sent to different rung partitions.
#[test]
fn test_a_take_starts_at_the_cursor_and_wraps() {
    let doorbell = Doorbell::new();
    for index in [2, 9, 14] {
        doorbell.ring(partition(index));
    }

    assert_eq!(doorbell.take(partition(9)), Some(partition(9)));
    assert_eq!(doorbell.take(partition(10)), Some(partition(14)));
    assert_eq!(doorbell.take(partition(15)), Some(partition(2)), "wrapped");
    assert_eq!(doorbell.take(partition(0)), None);
}

/// A missed ring rings everything: sixteen takes return sixteen partitions,
/// each once.
#[test]
fn test_ring_all_rings_every_partition_once() {
    let doorbell = Doorbell::new();
    doorbell.ring_all();

    let taken: Vec<u16> = (0..READY_PARTITIONS)
        .filter_map(|_| doorbell.take(partition(0)))
        .map(Partition::index)
        .collect();

    assert_eq!(taken, (0..READY_PARTITIONS).collect::<Vec<_>>());
    assert_eq!(doorbell.take(partition(0)), None);
}

/// Nothing rung is nothing to take, from any cursor.
#[test]
fn test_nothing_rung_takes_nothing() {
    for from in Partition::all() {
        assert_eq!(first_at_or_after(0, from), None);
        assert_eq!(Doorbell::new().take(from), None);
    }
}

/// Eight threads race for one rung bit, a thousand times over: exactly one
/// wins it each time, so one ring never sends two polls to the same peek.
#[test]
fn test_racing_takes_win_a_ring_once() {
    const RACERS: usize = 8;
    /// Enough rounds that a lost race would show, few enough to stay quick.
    const ROUNDS: usize = 1_000;
    for _ in 0..ROUNDS {
        let doorbell = Doorbell::new();
        doorbell.ring(partition(7));
        let start = Arc::new(Barrier::new(RACERS));
        let racers: Vec<_> = (0..RACERS)
            .map(|_| {
                let doorbell = doorbell.clone();
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    doorbell.take(partition(0))
                })
            })
            .collect();
        let won = racers
            .into_iter()
            .filter_map(|racer| racer.join().ok().flatten())
            .count();

        assert_eq!(won, 1, "one ring, one winner");
    }
}

/// A ring that lands after a poll took the partition is a new ring: the
/// take cleared only what it saw, so the next poll is sent there too.
#[test]
fn test_a_ring_after_a_take_is_a_new_ring() {
    let doorbell = Doorbell::new();
    doorbell.ring(partition(6));
    assert_eq!(doorbell.take(partition(0)), Some(partition(6)));

    doorbell.ring(partition(6));

    assert_eq!(doorbell.take(partition(0)), Some(partition(6)));
}

/// A mark whose ring was not published is reported once, naming its
/// partition and the datastore's own code, so an operator can tell a sick
/// slot from a bug.
#[test]
fn test_an_unrung_mark_is_reported_with_its_partition() {
    use afd_core::test_util::trace::Capture;

    use super::{EVENT_WAKE_RING_FAILED, report_unrung};

    /// The command the failure names.
    const COMMAND: &str = "SPUBLISH";
    let capture = Capture::install();

    report_unrung(partition(9), &crate::error::timed_out(COMMAND, 5));

    let line = capture.only(EVENT_WAKE_RING_FAILED);
    assert_eq!(line.field("partition"), Some("9"));
    assert!(
        line.field("cause_error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        line.field("reason")
            .is_some_and(|reason| reason.contains(COMMAND))
    );
}

#[cfg(feature = "test-util")]
mod listening {
    use super::partition;
    use crate::hub::SubscriptionHub;
    use crate::ready::Doorbell;
    use crate::ready::partition::ReadyPrefix;

    /// How long a test waits for the listener task to hear a frame.
    const HEARD_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

    /// Polls `doorbell` until `index` is rung or the wait runs out.
    async fn heard(doorbell: &Doorbell, index: u16) -> bool {
        let deadline = tokio::time::Instant::now() + HEARD_WITHIN;
        while tokio::time::Instant::now() < deadline {
            if doorbell.take(partition(index)) == Some(partition(index)) {
                return true;
            }
            tokio::task::yield_now().await;
        }
        false
    }

    /// A frame on a partition's channel rings that partition and no other.
    #[tokio::test]
    async fn test_a_frame_rings_its_partition() {
        let (hub, server) = SubscriptionHub::detached();
        let prefix = ReadyPrefix::production();
        let doorbell = Doorbell::new();
        doorbell.listen(&hub, &prefix);

        server.publish(&partition(11).wake_channel_under(&prefix), "11");

        assert!(heard(&doorbell, 11).await, "partition 11 was not rung");
        assert_eq!(doorbell.take(partition(0)), None, "only 11 was rung");
    }

    /// A gap on one channel rings every partition: the rings it lost cannot
    /// be named.
    #[tokio::test]
    async fn test_a_gap_rings_every_partition() {
        let (hub, server) = SubscriptionHub::detached();
        let prefix = ReadyPrefix::production();
        let doorbell = Doorbell::new();
        doorbell.listen(&hub, &prefix);
        let channel = partition(3).wake_channel_under(&prefix);

        server.confirm(&channel);
        server.confirm(&channel);

        assert!(
            heard(&doorbell, 0).await,
            "the gap did not ring partition 0"
        );
        assert_eq!(doorbell.take(partition(15)), Some(partition(15)));
    }

    /// Over a datastore nobody answers on, a ring is reported where it fails
    /// and never raised: the mark it follows has already landed or failed on
    /// its own.
    #[tokio::test]
    async fn test_a_ring_nobody_answers_is_reported_never_raised() {
        use afd_core::clock::UnixMillis;
        use afd_core::test_util::trace::Capture;
        use afd_crypto::entropy::Entropy;

        use crate::ready::{EVENT_WAKE_RING_FAILED, ReadyIndex, ReadyToken};
        use crate::{Dragonfly, DragonflyConfig, DragonflyRole};

        /// Port 1 is reserved and unbound.
        const NOWHERE: &str = "redis://127.0.0.1:1";

        let capture = Capture::install();
        let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
            DragonflyRole::Default,
            NOWHERE.to_owned(),
        ))
        .unwrap_or_else(|_| unreachable!("a lazy handle opens no socket"));
        let token = ReadyToken::mint(&Entropy::new(), UnixMillis::from_millis(1_759_000_000_000))
            .unwrap_or_else(|_| unreachable!("the host has entropy"));

        ReadyIndex::new(queue).ring(partition(4), &token).await;

        let line = capture.only(EVENT_WAKE_RING_FAILED);
        assert_eq!(line.field("partition"), Some("4"));
    }
}
