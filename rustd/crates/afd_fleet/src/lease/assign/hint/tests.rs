//! The hint's three decisions that need no datastore: which dirty partition
//! a poll is sent to, whether its entry outlives the poll, and how two peeks
//! share the scan.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::ready::READY_PARTITIONS;
use afd_dragonfly::{Dirty, Partition, Ready, ReadyToken};

use super::{Hint, MAX_READY_CANDIDATES_PER_POLL, PollCost, interleave, keeps_entry, spread};

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

/// A fresh token minted at [`AT`].
fn token() -> ReadyToken {
    ReadyToken::mint(&Entropy::new(), AT).expect("the host has entropy")
}

fn ready(fleet_id: String) -> Ready {
    Ready {
        fleet_id,
        token: token(),
    }
}

fn partition(index: u16) -> Partition {
    Partition::new(index).expect("a partition below sixteen")
}

fn dirty(index: u16) -> Dirty {
    Dirty {
        partition: partition(index),
        token: token(),
    }
}

/// The partition each cursor position sends a poll to, over `directory`.
fn sent_to(directory: &[Dirty]) -> Vec<Option<u16>> {
    (0..READY_PARTITIONS)
        .map(|position| spread(directory.to_vec(), partition(position)))
        .map(|sent| sent.map(|dirty| dirty.partition.index()))
        .collect()
}

/// Consecutive polls take the oldest three entries in turn, so a herd of
/// polls splits three ways, and the fourth-oldest waits for one of them to be
/// spent.
#[test]
fn test_polls_take_the_oldest_three_entries_in_turn() {
    let sent = sent_to(&[7, 3, 12, 9, 1].map(dirty));

    assert!(
        sent.starts_with(&[Some(7), Some(3), Some(12), Some(7)]),
        "{sent:?}"
    );
    for oldest in [7, 3, 12] {
        assert!(
            sent.contains(&Some(oldest)),
            "{oldest} is reached: {sent:?}"
        );
    }
    assert!(
        sent.iter()
            .flatten()
            .all(|index| [7, 3, 12].contains(index)),
        "{sent:?}"
    );
}

/// A directory shorter than the spread shares the polls among what it holds,
/// and an empty one sends no poll anywhere.
#[test]
fn test_a_short_directory_bounds_the_spread() {
    let alternating = (0..READY_PARTITIONS)
        .map(|position| Some(if position.is_multiple_of(2) { 4 } else { 11 }))
        .collect::<Vec<_>>();

    assert!(sent_to(&[dirty(4)]).iter().all(|sent| *sent == Some(4)));
    assert_eq!(sent_to(&[dirty(4), dirty(11)]), alternating);
    assert!(sent_to(&[]).iter().all(Option::is_none));
}

/// A poll that leased from the dirty partition, while its peek held other
/// fleets, comes back to it: the others are still waiting there.
#[test]
fn test_a_partition_still_holding_work_keeps_its_entry() {
    let dirty = partition(5);
    let leased = fleet_in(dirty);

    assert!(keeps_entry(dirty, Some(2), Some(&leased)));
}

/// A poll that leased from the rotation's partition keeps the entry: the
/// candidate scan may have offered the rotation's fleet first, so the hinted
/// fleets were never tried.
#[test]
fn test_a_lease_from_the_rotation_keeps_the_hint() {
    let dirty = partition(5);
    let elsewhere = fleet_in(partition(9));

    assert!(keeps_entry(dirty, Some(3), Some(&elsewhere)));
}

/// A partition the poll tried and could not lease from spends its entry:
/// nothing leased, its only fleet leased, a stale entry over an empty
/// partition, or a peek that failed.
#[test]
fn test_a_dry_finished_or_unreadable_partition_spends_its_entry() {
    let dirty = partition(5);
    let here = fleet_in(dirty);
    let elsewhere = fleet_in(partition(9));

    assert!(
        !keeps_entry(dirty, Some(3), None),
        "nothing leased: every fleet offered was tried"
    );
    assert!(
        !keeps_entry(dirty, Some(1), Some(&here)),
        "its only fleet is the one now running"
    );
    assert!(
        !keeps_entry(dirty, Some(0), None),
        "a stale entry over an empty partition"
    );
    assert!(
        !keeps_entry(dirty, None, Some(&elsewhere)),
        "a failed peek is never come back to"
    );
    assert!(!keeps_entry(dirty, None, None));
}

/// The two peeks take turns, so the hinted partition is offered first and
/// the rotation's fleets still reach the scan.
#[test]
fn test_two_peeks_take_turns_in_the_scan() {
    let hinted: Vec<Ready> = (0..2).map(|n| ready(format!("hinted-{n}"))).collect();
    let rotation: Vec<Ready> = (0..3).map(|n| ready(format!("rotation-{n}"))).collect();

    let merged = interleave(hinted, rotation);

    let order: Vec<&str> = merged.iter().map(|entry| entry.fleet_id.as_str()).collect();
    assert_eq!(
        order,
        [
            "hinted-0",
            "rotation-0",
            "hinted-1",
            "rotation-1",
            "rotation-2"
        ]
    );
}

/// Two full peeks still offer at most the poll's ceiling, half from each.
#[test]
fn test_two_full_peeks_share_the_ceiling() {
    let full = |side: &str| -> Vec<Ready> {
        (0..MAX_READY_CANDIDATES_PER_POLL)
            .map(|n| ready(format!("{side}-{n}")))
            .collect()
    };

    let merged = interleave(full("hinted"), full("rotation"));

    assert_eq!(merged.len(), MAX_READY_CANDIDATES_PER_POLL);
    let hinted = merged
        .iter()
        .filter(|entry| entry.fleet_id.starts_with("hinted"))
        .count();
    assert_eq!(
        hinted,
        MAX_READY_CANDIDATES_PER_POLL / 2,
        "neither side crowds the other out"
    );
}

/// A delete the datastore refuses is logged once, naming the partition it
/// concerned, and still counted as the command the poll sent; the poll goes on
/// without raising it.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_a_refused_spend_is_logged_and_counted() {
    use afd_core::test_util::trace::Capture;

    use super::super::diagnostics::EVENT_HINT_DELETE_FAILED;
    use crate::lease::test_dead::{id, leases};

    let log = Capture::install();
    let mut cost = PollCost::default();
    let hint = Hint {
        dirty: dirty(5),
        found: Some(0),
    };

    leases()
        .spend_hint(&id(9), Some(hint), None, &mut cost)
        .await;

    assert_eq!(cost.dragonfly_commands, 1, "the refused delete was sent");
    let refused = log.only(EVENT_HINT_DELETE_FAILED);
    assert_eq!(refused.field("partition"), Some("5"));
    assert_eq!(refused.field("runner_id"), Some(id(9).as_str()));
}
