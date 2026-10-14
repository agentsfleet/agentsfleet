//! The doorbell's decisions in a poll that need no datastore: whether a rung
//! partition is rung again, and the order two peeks reach the scan in; and,
//! against a dead store, what a refused rung peek costs and what a dropped
//! poll gives back.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::{Partition, Ready, ReadyToken};

use super::{MAX_READY_CANDIDATES_PER_POLL, cursor_first, offered_untried};

/// The instant every fixture token is minted at.
const AT: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// How far the walk looks; sixteen partitions are reached long before it.
const SEARCH: u32 = 10_000;

/// A fleet id that lands in `partition`, found by walking a fixed spelling.
fn fleet_in(partition: Partition) -> Uuid7 {
    (0_u32..SEARCH)
        .map(|n| format!("0195b4ba-8d3a-7000-8abc-{n:012}"))
        .find(|fleet| Partition::of(fleet) == partition)
        .and_then(|fleet| Uuid7::parse(&fleet).ok())
        .expect("sixteen partitions are all reachable")
}

fn ready(fleet_id: String) -> Ready {
    Ready {
        fleet_id,
        token: ReadyToken::mint(&Entropy::new(), AT).expect("the host has entropy"),
    }
}

fn partition(index: u16) -> Partition {
    Partition::new(index).expect("a partition below sixteen")
}

/// A fleet the scan offered from the rung partition, behind the one this poll
/// leased, was never tried: the partition is rung again for the next poll.
#[test]
fn test_an_untried_offered_fleet_rings_again() {
    let rung = partition(5);

    assert!(offered_untried(
        rung,
        &[fleet_in(partition(9)), fleet_in(rung)]
    ));
}

/// Nothing untried from the rung partition leaves it clear: nothing leased or
/// the poll failed (no untried list at all), its only offered fleet was the
/// one leased, or every untried fleet sits in another partition. A fleet the
/// scan left out, running or not this runner's labels, is never in the list,
/// so it cannot hold the bell.
#[test]
fn test_nothing_untried_from_the_rung_partition_stays_clear() {
    let rung = partition(5);

    assert!(!offered_untried(rung, &[]), "nothing untried");
    assert!(
        !offered_untried(rung, &[fleet_in(partition(9)), fleet_in(partition(2))]),
        "only other partitions' fleets were left"
    );
}

/// The cursor's partition's fleets lead the offer and the rung one's follow,
/// so the scan claims a rotation visit's work before the bell's.
#[test]
fn test_the_cursor_partition_leads_the_offer() {
    let rotation: Vec<Ready> = (0..3).map(|n| ready(format!("rotation-{n}"))).collect();
    let rung: Vec<Ready> = (0..2).map(|n| ready(format!("rung-{n}"))).collect();

    let offer = cursor_first(rotation, rung);

    let order: Vec<&str> = offer
        .ready
        .iter()
        .map(|entry| entry.fleet_id.as_str())
        .collect();
    assert_eq!(
        order,
        ["rotation-0", "rotation-1", "rotation-2", "rung-0", "rung-1"]
    );
    assert_eq!(offer.from_cursor, 3);
}

/// Two full peeks are both offered whole, so nothing the bell found is
/// dropped before the scan decides which fleets are eligible.
#[test]
fn test_two_full_peeks_are_offered_whole() {
    let full = |side: &str| -> Vec<Ready> {
        (0..MAX_READY_CANDIDATES_PER_POLL)
            .map(|n| ready(format!("{side}-{n}")))
            .collect()
    };

    let offer = cursor_first(full("rotation"), full("rung"));

    assert_eq!(offer.ready.len(), 2 * MAX_READY_CANDIDATES_PER_POLL);
    assert_eq!(offer.from_cursor, MAX_READY_CANDIDATES_PER_POLL);
}

/// A rung peek the datastore refuses is logged once, naming the partition
/// the doorbell rang, and costs the two commands the poll sent; settled with
/// nothing untried, the ring stays clear, so the next poll is not sent back.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_a_refused_rung_peek_is_logged_and_left_clear() {
    use afd_core::test_util::trace::Capture;
    use afd_dragonfly::Doorbell;

    use super::super::diagnostics::EVENT_WAKE_PEEK_FAILED;
    use super::PollCost;
    use crate::lease::test_dead::{id, leases};

    let log = Capture::install();
    let doorbell = Doorbell::new();
    doorbell.ring(partition(5));
    let store = leases().with_doorbell(doorbell.clone());
    let mut cost = PollCost::default();

    let (offer, rung) = store.ready_this_poll(&id(9), &mut cost).await;
    store.ring_again(rung, &[]);

    assert!(offer.is_err(), "both peeks were refused");
    assert_eq!(cost.dragonfly_commands, 2, "both peeks were sent");
    let refused = log.only(EVENT_WAKE_PEEK_FAILED);
    assert_eq!(refused.field("partition"), Some("5"));
    assert_eq!(refused.field("runner_id"), Some(id(9).as_str()));
    assert_eq!(doorbell.take(partition(0)), None, "the ring stays clear");
}

/// A poll dropped before it settles its rung partition, its runner gone,
/// gives the ring back rather than losing it.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_a_dropped_poll_gives_its_ring_back() {
    use afd_dragonfly::Doorbell;

    use super::PollCost;
    use crate::lease::test_dead::{id, leases};

    let doorbell = Doorbell::new();
    doorbell.ring(partition(5));
    let store = leases().with_doorbell(doorbell.clone());
    let mut cost = PollCost::default();

    let (_offer, rung) = store.ready_this_poll(&id(9), &mut cost).await;
    assert!(!doorbell.is_rung(partition(5)), "the poll took the ring");
    drop(rung);

    assert!(
        doorbell.is_rung(partition(5)),
        "the unsettled ring came back"
    );
}
