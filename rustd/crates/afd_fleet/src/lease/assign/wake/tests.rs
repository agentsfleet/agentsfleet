//! The doorbell's decisions in a poll that need no datastore: whether a rung
//! partition is rung again, and the order two peeks reach the scan in; and,
//! against a dead store, what a refused rung peek costs.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::{Partition, Ready, ReadyToken};

use super::{MAX_READY_CANDIDATES_PER_POLL, cursor_first, rings_again};

/// The instant every fixture token is minted at.
const AT: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// How far the walk looks; sixteen partitions are reached long before it.
const SEARCH: u32 = 10_000;

/// A fleet id that lands in `partition`, found by walking a fixed spelling.
fn fleet_in(partition: Partition) -> String {
    (0_u32..SEARCH)
        .map(|n| format!("0195b4ba-8d3a-7000-8abc-{n:012}"))
        .find(|fleet| Partition::of(fleet) == partition)
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

/// A poll that leased from the rung partition, while its peek held other
/// fleets, rings it again: the others are still waiting there.
#[test]
fn test_a_partition_still_holding_work_rings_again() {
    let rung = partition(5);
    let leased = fleet_in(rung);

    assert!(rings_again(rung, Some(2), Some(&leased)));
}

/// A poll that leased from the rotation's partition rings the rung one again:
/// the candidate scan may have offered the rotation's fleet first, so the
/// rung fleets were never tried.
#[test]
fn test_a_lease_from_the_rotation_rings_again() {
    let rung = partition(5);
    let elsewhere = fleet_in(partition(9));

    assert!(rings_again(rung, Some(3), Some(&elsewhere)));
}

/// A partition the poll tried and could not lease from stays clear: nothing
/// leased, its only fleet leased, a ring over an empty partition, or a peek
/// that failed.
#[test]
fn test_a_dry_finished_or_unreadable_partition_stays_clear() {
    let rung = partition(5);
    let here = fleet_in(rung);
    let elsewhere = fleet_in(partition(9));

    assert!(
        !rings_again(rung, Some(3), None),
        "nothing leased: every fleet offered was tried"
    );
    assert!(
        !rings_again(rung, Some(1), Some(&here)),
        "its only fleet is the one now running"
    );
    assert!(
        !rings_again(rung, Some(0), None),
        "a ring over an empty partition"
    );
    assert!(
        !rings_again(rung, Some(0), Some(&elsewhere)),
        "an empty rung peek has nothing to come back for, wherever the poll leased"
    );
    assert!(
        !rings_again(rung, None, Some(&elsewhere)),
        "a failed peek is never come back to"
    );
    assert!(!rings_again(rung, None, None));
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
/// the doorbell rang, and costs the two commands the poll sent; the ring is
/// not rung again, so the next poll is not sent back to it.
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
    store.ring_again(rung, None);

    assert!(offer.is_err(), "both peeks were refused");
    assert_eq!(cost.dragonfly_commands, 2, "both peeks were sent");
    let refused = log.only(EVENT_WAKE_PEEK_FAILED);
    assert_eq!(refused.field("partition"), Some("5"));
    assert_eq!(refused.field("runner_id"), Some(id(9).as_str()));
    assert_eq!(doorbell.take(partition(0)), None, "the ring stays clear");
}
