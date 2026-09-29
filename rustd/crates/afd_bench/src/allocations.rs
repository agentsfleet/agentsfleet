//! A global allocator that counts, for the lanes that report allocation cost.
//!
//! # Installed by the binary, read by the library
//!
//! Only one global allocator may exist per process, and a library that
//! declared one would impose it on every binary and test linking the crate. So
//! this module provides the TYPE and the counters it feeds; a lane binary (or
//! a test binary) opts in with
//! `#[global_allocator] static COUNTING: Counting = Counting;`.
//!
//! # An uninstalled counter reports nothing, never zero
//!
//! Where no binary installed it, the counters never move, and a lane dividing
//! a delta of zero by its frame count would report "zero allocations per
//! frame" — a number nobody measured. [`installed`] makes one allocation and
//! looks for it, and a lane writes allocation figures only when it is there.
//!
//! # Relaxed, on purpose
//!
//! The counters are read as deltas across a window that the reader bounds
//! with task joins, and those joins already order every allocation inside the
//! window before the read. Stronger orderings would cost every allocation in
//! the process a fence for no reading that needs one.

use core::alloc::{GlobalAlloc, Layout};
use std::alloc::System;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Every allocation and reallocation since the process started.
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// Bytes currently allocated and not yet freed.
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

/// The system allocator, counting as it goes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counting;

// SAFETY: every method forwards to `System` with the caller's pointer and
// layout unchanged, so `System`'s guarantees are this allocator's guarantees;
// the only addition is arithmetic on two atomics, which cannot allocate and
// cannot unwind.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for
        // `layout`, which is exactly the contract `System::alloc` requires.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: as `alloc` — the caller's contract is `System`'s.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` was returned by this allocator — which is to say by
        // `System` — for this `layout`, as the caller guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        LIVE_BYTES.fetch_add(new_size, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` for `layout`, and `new_size` meets
        // `GlobalAlloc::realloc`'s requirements, all as the caller guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// The two counters at one instant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Allocations and reallocations so far.
    pub allocations: u64,
    /// Bytes allocated and not yet freed.
    pub live_bytes: u64,
}

impl Snapshot {
    /// The counters as they stand now.
    #[must_use]
    pub fn now() -> Self {
        Self {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            live_bytes: u64::try_from(LIVE_BYTES.load(Ordering::Relaxed)).unwrap_or(u64::MAX),
        }
    }

    /// Allocations made since an earlier snapshot.
    #[must_use]
    pub const fn allocations_since(self, earlier: Self) -> u64 {
        self.allocations.saturating_sub(earlier.allocations)
    }

    /// Live bytes gained since an earlier snapshot, or zero if they shrank.
    #[must_use]
    pub const fn bytes_gained_since(self, earlier: Self) -> u64 {
        self.live_bytes.saturating_sub(earlier.live_bytes)
    }
}

/// Whether a binary installed [`Counting`] as this process's allocator.
///
/// Makes one heap allocation and answers whether the counter saw it.
#[must_use]
pub fn installed() -> bool {
    let before = Snapshot::now();
    let probe = std::hint::black_box(Box::new(0_u64));
    let after = Snapshot::now();
    drop(probe);
    after.allocations_since(before) > 0
}

#[cfg(test)]
mod tests;
