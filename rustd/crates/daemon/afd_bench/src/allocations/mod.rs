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
        allocated(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for
        // `layout`, which is exactly the contract `System::alloc` requires.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        // SAFETY: as `alloc` — the caller's contract is `System`'s.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        freed(layout.size());
        // SAFETY: `ptr` was returned by this allocator — which is to say by
        // `System` — for this `layout`, as the caller guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        freed(layout.size());
        allocated(new_size);
        // SAFETY: `ptr` came from `System` for `layout`, and `new_size` meets
        // `GlobalAlloc::realloc`'s requirements, all as the caller guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// One allocation of `size` bytes, or the new half of a reallocation.
fn allocated(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    LIVE_BYTES.fetch_add(size, Ordering::Relaxed);
    #[cfg(test)]
    this_thread::allocated(size);
}

/// `size` bytes handed back, or the old half of a reallocation.
fn freed(size: usize) {
    LIVE_BYTES.fetch_sub(size, Ordering::Relaxed);
    #[cfg(test)]
    this_thread::freed(size);
}

/// The same tally kept per thread, in the lib's own test build only.
///
/// The process-wide counters are what a lane reads, because a lane's work runs
/// on every runtime worker. A unit test cannot read them for an exact figure:
/// the harness runs the crate's other tests on other threads at the same
/// time, and one of them freeing memory between two snapshots moves the live
/// byte count under the test's own allocation. The test thread's tally is the
/// only one nothing else writes to.
#[cfg(test)]
mod this_thread {
    use core::cell::Cell;

    thread_local! {
        static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
        static LIVE_BYTES: Cell<usize> = const { Cell::new(0) };
    }

    // `try_with`, because an allocator may be called while a thread's locals
    // are being torn down, and a tally missed then is no test's business.
    // Wrapping, because a thread can free what another thread allocated.
    pub(super) fn allocated(size: usize) {
        ALLOCATIONS
            .try_with(|count| count.set(count.get().wrapping_add(1)))
            .unwrap_or_default();
        LIVE_BYTES
            .try_with(|bytes| bytes.set(bytes.get().wrapping_add(size)))
            .unwrap_or_default();
    }

    pub(super) fn freed(size: usize) {
        LIVE_BYTES
            .try_with(|bytes| bytes.set(bytes.get().wrapping_sub(size)))
            .unwrap_or_default();
    }

    /// This thread's allocations and live bytes; compare bytes with
    /// `wrapping_sub`, since the tally can wrap below zero.
    pub(super) fn tally() -> (u64, usize) {
        (
            ALLOCATIONS.try_with(Cell::get).unwrap_or_default(),
            LIVE_BYTES.try_with(Cell::get).unwrap_or_default(),
        )
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
