//! The budget's two counters: a fixed table of lease slots, and one packed
//! word for the current second.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// A slot no lease holds.
const VACANT: u64 = 0;

/// One lease's slot: whose it is, and how many spans it has kept.
#[derive(Debug, Default)]
pub(super) struct Slot {
    key: AtomicU64,
    spans: AtomicU32,
}

impl Slot {
    /// Counts one more span against this lease, when it has room under
    /// `limit`. Never wraps: a full lease stays full.
    pub(super) fn reserve(&self, limit: u32) -> bool {
        self.spans
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < limit).then_some(used + 1)
            })
            .is_ok()
    }

    /// Hands back a reservation the second refused.
    pub(super) fn unreserve(&self) {
        self.spans.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A fixed set of lease slots, claimed and freed by compare-and-swap.
///
/// Probed from the slot a key hashes to, so the common lookup touches one
/// slot; a full scan is the worst case and the table is a few hundred words.
#[derive(Debug)]
pub(super) struct LeaseTable {
    slots: Box<[Slot]>,
}

impl LeaseTable {
    /// A table of `capacity` vacant slots.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: (0..capacity.max(1)).map(|_slot| Slot::default()).collect(),
        }
    }

    /// Claims a slot for `key` and counts its root. `false` when every slot is
    /// held.
    pub(super) fn open(&self, key: u64) -> bool {
        self.probe(key).any(|slot| {
            let claimed = slot
                .key
                .compare_exchange(VACANT, key, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();
            if claimed {
                slot.spans.store(1, Ordering::Release);
            }
            claimed
        })
    }

    /// The slot `key` holds.
    pub(super) fn find(&self, key: u64) -> Option<&Slot> {
        self.probe(key)
            .find(|slot| slot.key.load(Ordering::Acquire) == key)
    }

    /// Frees the slot `key` holds.
    pub(super) fn close(&self, key: u64) {
        if let Some(slot) = self.find(key) {
            slot.spans.store(0, Ordering::Relaxed);
            slot.key.store(VACANT, Ordering::Release);
        }
    }

    /// How many slots a lease holds.
    #[cfg(test)]
    pub(super) fn held(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.key.load(Ordering::Acquire) != VACANT)
            .count()
    }

    /// Every slot, starting at the one `key` hashes to.
    fn probe(&self, key: u64) -> impl Iterator<Item = &Slot> {
        let length = u64::try_from(self.slots.len()).unwrap_or(u64::MAX);
        let start = usize::try_from(key % length).unwrap_or(0);
        self.slots
            .iter()
            .skip(start)
            .chain(self.slots.iter().take(start))
    }
}

/// The current second and how many spans it has admitted, as one word.
#[derive(Debug, Default)]
pub(super) struct SecondWindow(AtomicU64);

impl SecondWindow {
    /// Admits one span in second `now`, when the second has room under
    /// `limit`. A later second starts a fresh count; a second some other
    /// worker already moved past counts against the newer one, so the window
    /// never runs backwards.
    pub(super) fn take(&self, now: u32, limit: u32) -> bool {
        self.0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |packed| {
                let (second, used) = unpack(packed);
                let (second, used) = if now > second {
                    (now, 0)
                } else {
                    (second, used)
                };
                (used < limit).then(|| pack(second, used + 1))
            })
            .is_ok()
    }
}

/// `second` and `used` in one word: the second high, the count low.
fn pack(second: u32, used: u32) -> u64 {
    (u64::from(second) << 32) | u64::from(used)
}

/// The two halves [`pack`] joined.
fn unpack(packed: u64) -> (u32, u32) {
    let second = u32::try_from(packed >> 32).unwrap_or(u32::MAX);
    let used = u32::try_from(packed & u64::from(u32::MAX)).unwrap_or(u32::MAX);
    (second, used)
}
