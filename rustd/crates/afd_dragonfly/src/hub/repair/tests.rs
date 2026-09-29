//! The slot hash and the range lookup a repair groups channels by.

use super::{SLOTS, holds, slot};
use crate::topology::SlotRange;

/// The slots Dragonfly's `CLUSTER KEYSLOT` answered on the lane for these
/// names, and Redis's documented ones for the rest.
#[test]
fn slots_match_what_the_cluster_computes() {
    assert_eq!(slot("foo"), 12_182);
    assert_eq!(slot("bar"), 5_061);
    assert_eq!(slot("fleet:afdt37598_0_node-1:activity"), 586);
    assert_eq!(slot("fleet:afdt37598_0_node-0:activity"), 11_535);
}

/// A hash tag decides the slot when it has content, and not otherwise.
#[test]
fn a_hash_tag_decides_the_slot_only_when_it_has_content() {
    assert_eq!(slot("{user1000}.following"), slot("{user1000}.followers"));
    assert_eq!(slot("{user1000}.following"), slot("user1000"));
    assert_ne!(
        slot("foo{}{bar}"),
        slot("bar"),
        "an empty first tag hashes the whole key"
    );
    assert_eq!(slot("foo{{bar}}zap"), slot("{bar"));
    assert!(slot("{unclosed") < SLOTS);
}

/// A range holds its first and last slot and nothing outside them.
#[test]
fn a_range_holds_exactly_its_own_slots() {
    let range = SlotRange {
        first: 8_192,
        last: 16_000,
        id: Some("dfly-b".to_owned()),
    };
    assert!(holds(&range, 8_192) && holds(&range, 16_000));
    assert!(!holds(&range, 8_191) && !holds(&range, 16_001));
}
