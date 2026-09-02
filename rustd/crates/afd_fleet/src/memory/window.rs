//! Which memory entries a run is seeded with, and which stay in Postgres.
//!
//! Pure, deterministic, datastore-free. Summarisation belongs on the executor
//! plane; this decides only what FITS.
//!
//! # The rule, in full
//!
//! Rows arrive newest-first. Every `core` entry that fits the budget hydrates
//! before any non-core entry is considered; whatever budget survives fills with
//! the newest non-core entries. Each tier takes a PREFIX of its own order — the
//! first entry that overflows ends that tier, so an older, smaller entry never
//! slips in behind a rejected newer one. A non-empty input always hydrates at
//! least one entry, even if that entry alone busts the budget.
//!
//! # Why this is short
//!
//! `fleet_memory.zig` spends a `TierRun` struct with `used`/`head_taken`/
//! `closed` flags, an `admit` method, and TWO passes over the rows — one to
//! size the pinned tier and one to replay the identical decisions while
//! selecting. Its comment has to promise the passes stay "in lockstep by
//! construction", because nothing checks that two copies of one rule agree.
//!
//! "Take entries while the running total fits" is a prefix of a cumulative sum,
//! which is [`Iterator::scan`] and [`Iterator::take_while`]. Written that way
//! the flags are gone, the second pass is gone, and the rule appears once, in
//! [`fits`]. What is left is two counts and a countdown.

use afd_wire::memory::MemoryDelta;

/// The one category that hydrates before recency is considered.
///
/// Shared with [`crate::sql::EVICT_PAST_CAP`], which protects exactly
/// this category when choosing eviction victims. One declaration keeps the two
/// in lockstep — hydration must never pin what eviction deletes first, which is
/// what `fleet_memory.zig` needs a `comptime` assertion over a static map to
/// enforce.
pub const PINNED_CATEGORY: &str = "core";

/// Scratch notes, which expire on a retention sweep.
///
/// Every other category is expiry-exempt by construction: the sweep binds this
/// as a PARAMETER rather than matching a pattern, so a new category cannot
/// accidentally become perishable.
pub const DAILY_CATEGORY: &str = "daily";

/// The bytes one entry charges against any memory budget.
///
/// The single formula the hydration window, the push cap and the dropped-bytes
/// accounting all share, so the three cannot silently diverge.
#[must_use]
pub fn entry_bytes(entry: &MemoryDelta<'_>) -> usize {
    entry.key.len() + entry.content.len() + entry.category.len()
}

/// Cumulative [`entry_bytes`] over a slice.
#[must_use]
pub fn total_bytes(entries: &[MemoryDelta<'_>]) -> usize {
    entries.iter().map(entry_bytes).sum()
}

/// Whether an entry hydrates ahead of recency.
fn is_pinned(entry: &MemoryDelta<'_>) -> bool {
    // Anything unrecognised is windowed. Custom categories are expected — the
    // column carries no CHECK — and the default must be the safe one: an
    // unknown string that pinned itself would starve the window and could never
    // be evicted.
    entry.category == PINNED_CATEGORY
}

/// How many leading entries of `sizes` fit `budget`.
///
/// The whole admission rule, in one place. `scan` carries the running total and
/// `take_while` ends the tier at the first overflow — which is what stops a
/// small old entry being admitted past a rejected large new one.
///
/// `head` grants the first entry a pass regardless of size. That is the
/// never-hydrate-nothing rule, and expressing it as `max` rather than as a flag
/// threaded through the loop is why there is no loop.
fn fits(sizes: impl Iterator<Item = usize>, budget: usize, head: bool) -> usize {
    let mut sizes = sizes.peekable();
    let non_empty = sizes.peek().is_some();
    let fitting = sizes
        .scan(0_usize, |used, size| {
            *used += size;
            Some(*used)
        })
        .take_while(|used| *used <= budget)
        .count();
    if head && non_empty {
        fitting.max(1)
    } else {
        fitting
    }
}

/// A hydration window: what the run is seeded with, and what stays behind.
///
/// Both halves are owned, so "the dropped set" is a value. The Zig compacts in
/// place and leaves the slice a permutation whose tail is the dropped set — a
/// fact its callers depend on and only a comment states.
#[derive(Debug)]
pub struct Window<'a> {
    /// The entries that hydrate, in their original recency order.
    pub kept: Vec<MemoryDelta<'a>>,
    /// The entries that stay durable in Postgres, unhydrated.
    pub dropped: Vec<MemoryDelta<'a>>,
}

impl Window<'_> {
    /// The bytes that did not hydrate.
    #[must_use]
    pub fn dropped_bytes(&self) -> usize {
        total_bytes(&self.dropped)
    }
}

/// Split `entries` into the hydration window and the tail left behind.
///
/// `entries` must arrive newest-first — the statement orders them, and the
/// prefix semantics are meaningless otherwise.
#[must_use]
pub fn select(entries: Vec<MemoryDelta<'_>>, budget: usize) -> Window<'_> {
    let window = select_counted(entries, budget);
    afd_observability::producers::memory::hydration_window(
        counted(window.kept.len()),
        counted(window.dropped.len()),
        counted(window.dropped_bytes()),
    );
    window
}

/// A count as the width the wire carries.
///
/// Saturating rather than truncating: a window past `u64::MAX` entries is not
/// a state this process can reach, and a wrapping cast would report a large
/// window as a tiny one.
fn counted(size: usize) -> u64 {
    u64::try_from(size).unwrap_or(u64::MAX)
}

/// [`select`] without the recording, so the window exists before anything is
/// said about it.
fn select_counted(entries: Vec<MemoryDelta<'_>>, budget: usize) -> Window<'_> {
    let sizes = |pinned: bool| {
        entries
            .iter()
            .filter(move |entry| is_pinned(entry) == pinned)
            .map(entry_bytes)
    };

    let pinned_count = fits(sizes(true), budget, true);
    let pinned_bytes: usize = sizes(true).take(pinned_count).sum();
    // Saturating: a pinned tier that filled the budget leaves the windowed tier
    // nothing, and a head entry that alone busted it leaves less than nothing.
    let windowed_budget = budget.saturating_sub(pinned_bytes);
    // The pinned head already satisfies never-hydrate-nothing, so the windowed
    // tier gets head privilege only when there is no pinned entry at all.
    let windowed_count = fits(sizes(false), windowed_budget, pinned_count == 0);

    // Each tier's admitted set is its first N, so selection is a countdown per
    // tier rather than a second evaluation of the rule.
    let (mut pinned_left, mut windowed_left) = (pinned_count, windowed_count);
    let (kept, dropped) = entries.into_iter().partition(|entry| {
        let left = if is_pinned(entry) {
            &mut pinned_left
        } else {
            &mut windowed_left
        };
        let admit = *left > 0;
        *left = left.saturating_sub(1);
        admit
    });
    Window { kept, dropped }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{PINNED_CATEGORY, entry_bytes, select};
    use afd_wire::memory::MemoryDelta;

    /// The budget these tests reason against.
    ///
    /// Small enough that a fixture can exceed it without allocating a quarter
    /// of a megabyte, and named rather than passed as a literal so each test
    /// reads as a statement about a budget rather than about a number.
    const BUDGET: usize = 1_024;

    /// An entry that CHARGES `bytes` against a budget.
    ///
    /// Sized by what [`entry_bytes`] will actually count, not by its content
    /// alone: the charge is key + content + category, so a fixture that sized
    /// only the content would charge more than the test asked for — and every
    /// case here is a statement about how a budget divides. Sizing the content
    /// to `bytes` and asserting on halves of the budget is how a test that
    /// looked exactly right came to expect two entries where one fits.
    ///
    /// Panics on a `bytes` too small to cover the key and category, which is a
    /// fixture that cannot express what it was asked for.
    fn entry(key: &'static str, bytes: usize, category: &'static str) -> MemoryDelta<'static> {
        let overhead = key.len() + category.len();
        let content = bytes
            .checked_sub(overhead)
            .expect("an entry cannot charge less than its own key and category");
        MemoryDelta {
            key: key.into(),
            content: "x".repeat(content).into(),
            category: category.into(),
        }
    }

    /// The keys a window kept, in order.
    fn kept_keys(entries: Vec<MemoryDelta<'static>>, budget: usize) -> Vec<String> {
        select(entries, budget)
            .kept
            .into_iter()
            .map(|entry| entry.key.into_owned())
            .collect()
    }

    /// Core entries hydrate before any non-core entry is considered.
    ///
    /// The rule the whole tier split exists for: `old-core` is the OLDEST entry
    /// here and still outranks two newer conversation entries, because pinning
    /// is not a recency question.
    #[test]
    fn test_core_entries_outrank_newer_non_core_ones() {
        let big = BUDGET / 2;
        let kept = kept_keys(
            vec![
                entry("new-chat", big, "conversation"),
                entry("mid-chat", big, "conversation"),
                entry("old-core", big, PINNED_CATEGORY),
            ],
            BUDGET,
        );
        assert!(
            kept.contains(&"old-core".to_owned()),
            "the oldest core entry hydrates ahead of newer chat: {kept:?}"
        );
        assert_eq!(
            kept.len(),
            2,
            "core takes half the budget, leaving room for exactly one chat entry: {kept:?}"
        );
    }

    /// Within a tier the window is a PREFIX, so a rejection ends that tier.
    ///
    /// The property that makes this a window rather than a knapsack: `small`
    /// would fit in the budget `huge` left behind, and is refused anyway,
    /// because admitting it would mean the window is no longer "the newest N
    /// bytes" but "whichever newest entries happen to pack".
    #[test]
    fn test_a_rejected_entry_ends_its_tier_rather_than_being_skipped() {
        let kept = kept_keys(
            vec![
                entry("newest", 32, "conversation"),
                entry("huge", BUDGET, "conversation"),
                entry("small", 32, "conversation"),
            ],
            BUDGET,
        );
        assert_eq!(
            kept,
            vec!["newest".to_owned()],
            "the tier closes at the first overflow; a later small entry does not sneak in"
        );
    }

    /// A non-empty set always hydrates something, even past the budget.
    ///
    /// A run seeded with nothing looks identical to a fleet that has learned
    /// nothing, and the two must not be confusable — so one oversized entry is
    /// admitted rather than yielding an empty window.
    #[test]
    fn test_one_oversized_entry_still_hydrates() {
        let huge = entry("enormous", BUDGET * 4, "conversation");
        assert!(
            entry_bytes(&huge) > BUDGET,
            "the fixture must exceed the budget for this test to mean anything"
        );
        let single = vec![huge];
        assert_eq!(
            kept_keys(single, BUDGET),
            vec!["enormous".to_owned()],
            "a fleet that remembers one huge thing hydrates it rather than nothing"
        );
    }

    /// Nothing in, nothing out — and no panic on the head rule.
    #[test]
    fn test_an_empty_set_hydrates_nothing() {
        let window = select(Vec::new(), BUDGET);
        assert!(window.kept.is_empty(), "no entries, no window");
        assert_eq!(window.dropped_bytes(), 0, "and nothing to account as lost");
    }

    /// Every entry lands in exactly one half, so nothing is lost or duplicated.
    ///
    /// The invariant the Zig states as a comment about its slice being "a
    /// permutation of its input". Here it is checkable, because both halves are
    /// values.
    #[test]
    fn test_the_two_halves_partition_the_input() {
        let entries = vec![
            entry("a", BUDGET, PINNED_CATEGORY),
            entry("b", BUDGET, "conversation"),
            entry("c", BUDGET, "daily"),
        ];
        let total = super::total_bytes(&entries);
        let window = select(entries, BUDGET);
        assert_eq!(
            window.kept.len() + window.dropped.len(),
            3,
            "every entry is kept or dropped, never both and never neither"
        );
        assert_eq!(
            super::total_bytes(&window.kept) + window.dropped_bytes(),
            total,
            "and the bytes account exactly, which is what the loss metric reports on"
        );
    }
}
