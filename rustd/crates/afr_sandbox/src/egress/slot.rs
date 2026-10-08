//! Egress slots: the index a live scope is named and addressed by, held by one
//! scope at a time across the whole process.
//!
//! The claims are one process-wide bitmap rather than a field of an engine:
//! every engine in a process names its links and tables from the same host
//! namespace, so two engines handing out the same index would meet each
//! other's objects. Claiming is a single atomic `fetch_or`, so no lock guards
//! it and a scope built on any thread claims its slot without waiting.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many scopes a host holds at once: one `/30` per slot, the slot being the
/// third octet of [`NETWORK`], `0..=253`.
pub(crate) const SLOTS: u8 = 254;
/// The host-side end of each slot's veth pair: `afv<slot>`.
pub(crate) const LINK_PREFIX: &str = "afv";
/// The sandbox-side end, named before it moves into the sandbox: `afp<slot>`.
pub(crate) const PEER_PREFIX: &str = "afp";
/// Each slot's `nf_tables` table, in the `inet` family: `afegress<slot>`.
pub(crate) const TABLE_PREFIX: &str = "afegress";
/// The first two octets every slot's `/30` sits under.
const NETWORK: [u8; 2] = [10, 69];
/// Each slot's network: the host side, the sandbox side, and nothing else.
pub(crate) const PREFIX_LEN: u8 = 30;
/// The host side's last octet in its slot's `/30`.
const HOST_OCTET: u8 = 1;
/// The sandbox side's last octet, whose default route is the host side.
const SANDBOX_OCTET: u8 = 2;
/// Bits in one word of the claim bitmap.
const WORD_BITS: u8 = 64;

/// One bit per slot, set while some scope in this process holds it: slots
/// `0..64`, then `64..128`, `128..192` and `192..254`.
static CLAIMED: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// One slot's index, and every name and address derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot(u8);

impl Slot {
    /// The slot `index`, when there is one.
    pub(crate) fn new(index: u8) -> Option<Self> {
        (index < SLOTS).then_some(Self(index))
    }

    /// The slot a link or table named `name` belongs to, when `name` is
    /// `prefix` followed by a slot's index and nothing else.
    pub(crate) fn named(name: &str, prefix: &str) -> Option<Self> {
        let digits = name.strip_prefix(prefix)?;
        // `afv07` is no name this crate makes; reading it as slot 7 would let
        // the sweep remove another program's link.
        let canonical = !digits.starts_with('0') || digits == "0";
        digits
            .parse()
            .ok()
            .filter(|_| canonical)
            .and_then(Self::new)
    }

    /// Its index, as a log line names it.
    pub(crate) const fn index(self) -> u8 {
        self.0
    }

    /// The host-side end of its veth pair.
    pub(crate) fn link(self) -> String {
        format!("{LINK_PREFIX}{}", self.0)
    }

    /// The sandbox-side end of its veth pair.
    pub(crate) fn peer(self) -> String {
        format!("{PEER_PREFIX}{}", self.0)
    }

    /// Its `nf_tables` table.
    pub(crate) fn table(self) -> String {
        format!("{TABLE_PREFIX}{}", self.0)
    }

    /// Its `/30`'s network address.
    pub(crate) const fn network(self) -> Ipv4Addr {
        self.address(0)
    }

    /// The host side's address.
    pub(crate) const fn host(self) -> Ipv4Addr {
        self.address(HOST_OCTET)
    }

    /// The sandbox side's address.
    pub(crate) const fn sandbox(self) -> Ipv4Addr {
        self.address(SANDBOX_OCTET)
    }

    const fn address(self, last: u8) -> Ipv4Addr {
        Ipv4Addr::new(NETWORK[0], NETWORK[1], self.0, last)
    }

    /// Its bit in the claim bitmap: the word, and the mask within it.
    fn bit(self) -> (&'static AtomicU64, u64) {
        let [first, second, third, fourth] = &CLAIMED;
        let word = match self.0 / WORD_BITS {
            0 => first,
            1 => second,
            2 => third,
            _ => fourth,
        };
        (word, 1 << (self.0 % WORD_BITS))
    }
}

/// A slot this process holds; released when dropped.
#[derive(Debug)]
pub(crate) struct Claim(Slot);

impl Claim {
    /// The lowest slot no scope in this process holds, held from now on.
    pub(crate) fn any() -> Option<Self> {
        (0..SLOTS).filter_map(Slot::new).find_map(Self::exactly)
    }

    /// `slot`, held from now on, unless a scope in this process holds it.
    pub(crate) fn exactly(slot: Slot) -> Option<Self> {
        let (word, mask) = slot.bit();
        let before = word.fetch_or(mask, Ordering::AcqRel);
        // Built only when this call set the bit: a claim built and dropped
        // on the failing path would clear the bit its holder still owns.
        (before & mask == 0).then(|| Self(slot))
    }

    /// The slot held.
    pub(crate) const fn slot(&self) -> Slot {
        self.0
    }

    /// Holds the slot for the life of the process: its link or its table could
    /// not be removed, and a scope reusing its name would meet them. The boot
    /// sweep of the next run removes them.
    pub(crate) fn abandon(self) {
        std::mem::forget(self);
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        let (word, mask) = self.0.bit();
        word.fetch_and(!mask, Ordering::AcqRel);
    }
}

/// Taken by every test that claims slots, so the one that holds them all
/// never starves another: the bitmap is the whole test process's.
#[cfg(test)]
pub(crate) static TEST_CLAIMS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// [`TEST_CLAIMS`], held for a test's length.
#[cfg(test)]
pub(crate) fn claims_held() -> std::sync::MutexGuard<'static, ()> {
    TEST_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[path = "slot_tests.rs"]
mod tests;
