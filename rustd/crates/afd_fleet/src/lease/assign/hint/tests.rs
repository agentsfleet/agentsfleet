//! The hint's two decisions that need no datastore: whether a dirty
//! partition's entry outlives a poll, and how two peeks share the scan.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::{Partition, Ready, ReadyToken};

use super::{MAX_READY_CANDIDATES_PER_POLL, interleave, keeps_entry};

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
