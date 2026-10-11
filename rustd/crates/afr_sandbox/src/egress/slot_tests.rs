#![expect(
    clippy::unwrap_used,
    reason = "test module: a slot the test names exists"
)]

use std::net::Ipv4Addr;

use super::{Claim, LINK_PREFIX, SLOTS, Slot, Slots, TABLE_PREFIX};

/// Every name and address a slot carries is derived from its index, so the
/// builder, the sweep and the tests can never disagree on one.
#[test]
fn test_a_slot_names_and_addresses_its_scope() {
    let slot = Slot::new(12).unwrap();

    assert_eq!(
        (slot.link(), slot.peer(), slot.table()),
        (
            "afv12".to_owned(),
            "afp12".to_owned(),
            "afegress12".to_owned()
        )
    );
    assert_eq!(
        (slot.network(), slot.host(), slot.sandbox()),
        (
            Ipv4Addr::new(10, 69, 12, 0),
            Ipv4Addr::new(10, 69, 12, 1),
            Ipv4Addr::new(10, 69, 12, 2)
        )
    );
}

/// No slot past the last `/30` the third octet holds.
#[test]
fn test_slots_stop_at_the_last_network() {
    assert!(Slot::new(SLOTS - 1).is_some());
    assert!(Slot::new(SLOTS).is_none());
}

/// The sweep reads back only names this crate makes: the prefix, then a slot's
/// index written plainly.
#[test]
fn test_only_names_this_crate_makes_read_as_slots() {
    assert_eq!(Slot::named("afv0", LINK_PREFIX), Slot::new(0));
    assert_eq!(Slot::named("afegress253", TABLE_PREFIX), Slot::new(253));
    for foreign in ["afv", "afv07", "afv254", "afv1x", "eth0", "afegress-1"] {
        assert_eq!(Slot::named(foreign, LINK_PREFIX), None, "{foreign}");
    }
}

/// A slot is held by one claim at a time and free again once that claim is
/// dropped; an abandoned one stays held.
#[test]
fn test_a_slot_is_held_by_one_claim_at_a_time() {
    let pool = Slots::fresh();
    let slot = Slot::new(250).unwrap();

    let held = Claim::exactly(pool, slot).unwrap();
    assert!(
        Claim::exactly(pool, slot).is_none(),
        "a second claim waits its turn"
    );
    assert!(
        Claim::exactly(pool, slot).is_none(),
        "and a refused claim frees nothing"
    );
    assert_eq!(held.slot(), slot);
    drop(held);
    let again = Claim::exactly(pool, slot);
    assert!(again.is_some(), "free once released");
    drop(again);

    let kept = Slot::new(251).unwrap();
    Claim::exactly(pool, kept).unwrap().abandon();
    assert!(Claim::exactly(pool, kept).is_none(), "abandoned for good");
}

/// Claims made at once from many threads never hand one slot out twice, and
/// every thread is answered: sixty-four claims fit in the pool.
#[test]
fn test_concurrent_claims_never_share_a_slot() {
    let pool = Slots::fresh();
    let claims: Vec<Claim> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..16)
            .map(|_| scope.spawn(|| (0..4).filter_map(|_| Claim::any(pool)).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });

    let mut slots: Vec<u8> = claims.iter().map(|claim| claim.slot().index()).collect();
    let claimed = slots.len();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(claimed, 64, "no claim is refused while slots are free");
    assert_eq!(slots.len(), claimed, "every claim holds its own slot");
}

/// Two pools hold the same slot independently: a fake kernel's claims never
/// touch the running kernel's, nor another fake's.
#[test]
fn test_each_pool_holds_its_own_slots() {
    let slot = Slot::new(0).unwrap();
    let (one, other) = (Slots::fresh(), Slots::fresh());

    let held = Claim::exactly(one, slot).unwrap();

    assert!(Claim::exactly(one, slot).is_none());
    assert!(Claim::exactly(other, slot).is_some());
    drop(held);
}

/// The last slot claimed, the pool answers no more.
#[test]
fn test_a_full_pool_refuses_the_next_claim() {
    let pool = Slots::fresh();
    let held: Vec<Claim> = std::iter::from_fn(|| Claim::any(pool)).collect();

    assert_eq!(held.len(), usize::from(SLOTS));
    assert_eq!(
        held.last().map(|claim| claim.slot().index()),
        Some(SLOTS - 1)
    );
    assert!(Claim::any(pool).is_none());
}
