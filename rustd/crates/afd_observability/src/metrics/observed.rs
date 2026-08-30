//! A value a collection callback may read without doing any work.
//!
//! # Why a snapshot cell exists at all
//!
//! The SDK invokes observable callbacks under its own pipeline lock, with no
//! `catch_unwind` and no timeout. A callback that touches Redis, reads `/proc`,
//! or takes a lock some other thread holds does not slow one metric down — it
//! stalls or poisons the entire metrics pipeline, and the first symptom is
//! every family going silent at once.
//!
//! So nothing is read INSIDE a callback. A publisher writes what it last saw
//! into one of these cells on its own cadence, and the callback only loads.
//!
//! # Absent is not zero
//!
//! A cell that was never written, or whose publisher last failed, declines to
//! observe rather than reporting `0`. That preserves the rule the Zig daemon
//! ran on: a failed read is ABSENT, and a gap in a graph is the truth. A zero
//! is a claim — "the queue is empty", "no runners are leased" — and publishing
//! it from a failed read invents an operational fact that no one measured.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(test)]
mod tests;

/// A `u64` reading published for a collection callback to load.
///
/// The validity flag and the value are separate atomics rather than one packed
/// word because the value uses the full `u64` range: a sentinel would collide
/// with a real reading, and `u64::MAX` bytes is a number a real gauge can hold.
#[derive(Debug, Default)]
pub struct Observed {
    value: AtomicU64,
    valid: AtomicBool,
}

impl Observed {
    /// A cell nothing has published into yet, which observes nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
            valid: AtomicBool::new(false),
        }
    }

    /// Publishes a reading a callback may now observe.
    ///
    /// `Release` on the flag pairs with the `Acquire` in [`Observed::load`], so
    /// a callback that sees the cell valid also sees the value that went with
    /// it. Written in this order for that reason: the value first, then the
    /// flag that advertises it.
    pub fn publish(&self, reading: u64) {
        self.value.store(reading, Ordering::Relaxed);
        self.valid.store(true, Ordering::Release);
    }

    /// Withdraws the cell, so the callback observes nothing until the next
    /// successful publish.
    ///
    /// This is what a publisher calls when its own read failed. It is NOT
    /// `publish(0)`, and the difference is the whole point: a withdrawn cell
    /// leaves a gap, and a zero would leave a measurement nobody took.
    pub fn withdraw(&self) {
        self.valid.store(false, Ordering::Release);
    }

    /// The reading, or nothing when no publisher has vouched for one.
    ///
    /// This is everything a callback is allowed to do. It takes no lock,
    /// allocates nothing, and cannot fail, which is what makes it safe to run
    /// under the SDK's pipeline lock.
    #[must_use]
    pub fn load(&self) -> Option<u64> {
        self.valid
            .load(Ordering::Acquire)
            .then(|| self.value.load(Ordering::Relaxed))
    }
}
