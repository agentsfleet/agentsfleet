//! The partition function and the cursor, which decide something without a
//! datastore: where a mark lands, and the order a poll visits the hashes.

use std::collections::BTreeSet;

use super::{Partition, READY_INDEX_KEY, READY_PARTITIONS, ReadyCursor};

/// The same fleet lands in the same partition every time, and every
/// partition is one the index knows.
#[test]
fn a_fleet_always_lands_in_the_same_partition_and_never_past_the_last() {
    for n in 0..1_000_u32 {
        let fleet = format!("0199a0b0-0000-7000-8000-{n:012}");
        let first = Partition::of(&fleet);
        assert_eq!(first, Partition::of(&fleet), "{fleet} moved between calls");
        assert!(
            Partition::new(first.index()).is_some(),
            "{fleet} landed past the last partition: {}",
            first.index()
        );
    }
}

/// Fleets spread over every partition rather than piling into a few: a
/// population that fits in one hash would defeat the reason there are
/// several.
#[test]
fn a_population_reaches_every_partition() {
    let reached: BTreeSet<u16> = (0..1_000_u32)
        .map(|n| Partition::of(&format!("fleet-{n}")).index())
        .collect();
    assert_eq!(
        reached.len(),
        usize::from(READY_PARTITIONS),
        "a thousand fleets left a partition empty: {reached:?}"
    );
}

/// The key carries the partition number as its hash tag and nothing else in
/// braces, so the number alone decides the slot.
#[test]
fn every_key_is_the_shared_prefix_plus_its_own_hash_tag() {
    let keys: BTreeSet<String> = Partition::all().map(Partition::key).collect();
    assert_eq!(
        keys.len(),
        usize::from(READY_PARTITIONS),
        "two partitions share a key"
    );
    for partition in Partition::all() {
        let key = partition.key();
        let tag = format!("{{{}}}", partition.index());
        assert_eq!(
            key,
            format!("{READY_INDEX_KEY}:{tag}"),
            "unexpected key shape"
        );
        assert_eq!(key.matches('{').count(), 1, "{key} carries a second brace");
    }
}

/// A number past the last partition is refused, not wrapped.
#[test]
fn a_partition_past_the_last_is_refused() {
    assert!(Partition::new(READY_PARTITIONS).is_none());
    assert!(Partition::new(u16::MAX).is_none());
    assert!(Partition::new(READY_PARTITIONS - 1).is_some());
}

/// One rotation of the cursor visits every partition exactly once, in order.
#[test]
fn one_rotation_visits_every_partition_once() {
    let cursor = ReadyCursor::new();
    let visited: Vec<u16> = (0..READY_PARTITIONS)
        .map(|_| cursor.advance().index())
        .collect();
    let expected: Vec<u16> = (0..READY_PARTITIONS).collect();
    assert_eq!(
        visited, expected,
        "the rotation skipped or repeated a partition"
    );
}

/// The counter's wrap is invisible: the partition after the top of the
/// range is the one after the top's, not a jump back to some other start.
#[test]
fn the_cursor_continues_its_rotation_across_the_counter_seam() {
    let cursor = ReadyCursor::starting_at(u16::MAX - 1);
    let before_seam = cursor.advance().index();
    let at_seam = cursor.advance().index();
    let after_seam = cursor.advance().index();
    assert_eq!(at_seam, (before_seam + 1) % READY_PARTITIONS);
    assert_eq!(after_seam, (at_seam + 1) % READY_PARTITIONS);
}

/// Every clone turns the same rotation, which is what makes the cursor one
/// per process rather than one per handle.
#[test]
fn clones_share_one_rotation() {
    let cursor = ReadyCursor::new();
    let twin = cursor.clone();
    let first = cursor.advance().index();
    let second = twin.advance().index();
    assert_eq!(
        second,
        (first + 1) % READY_PARTITIONS,
        "a clone restarted the rotation"
    );
}
