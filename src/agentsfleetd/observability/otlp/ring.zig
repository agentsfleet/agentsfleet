//! Generic lock-free MPSC ring buffer for the OTLP exporters.
//!
//! Many producers push (the request/worker threads, fire-and-forget); a single
//! consumer (the exporter's flush thread) pops. `Entry` is a fixed-size value
//! copied into the ring — no heap, no per-entry allocation. Extracted from the
//! byte-identical rings otel_traces / otel_logs / otel_metrics each carried.

const std = @import("std");

pub fn Ring(comptime Entry: type, comptime capacity: usize) type {
    return struct {
        const Self = @This();

        pub const CAPACITY = capacity;
        pub const EntryType = Entry;

        // SAFETY: each slot is written by exactly one claiming producer before
        // that slot's ready flag is published; pop reads only ready slots.
        buffer: [capacity]Entry = undefined,
        ready: [capacity]std.atomic.Value(u8) = [_]std.atomic.Value(u8){std.atomic.Value(u8).init(0)} ** capacity,
        head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
        tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
        dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

        pub fn push(self: *Self, entry: Entry) bool {
            while (true) {
                const head = self.head.load(.acquire);
                const tail = self.tail.load(.acquire);
                const next_head = (head + 1) % capacity;
                if (next_head == tail) {
                    // safe because: independent statistic; no ordering required.
                    _ = self.dropped.fetchAdd(1, .monotonic);
                    return false;
                }
                // safe because: the .acq_rel cmpxchg claims slot `head`
                // exclusively — a losing producer observes the new head and
                // retries, so two producers can never write the same slot.
                // Failure order .acquire re-reads a coherent head.
                if (self.head.cmpxchgWeak(head, next_head, .acq_rel, .acquire)) |_| continue;
                self.buffer[head] = entry;
                // safe because: .release publishes the completed slot write to
                // pop()'s .acquire load of the same flag.
                self.ready[head].store(1, .release);
                return true;
            }
        }

        pub fn pop(self: *Self) ?Entry {
            // safe because: tail is consumer-owned (single flush thread); head's
            // .acquire pairs with producers' claim cmpxchg.
            const tail = self.tail.load(.acquire);
            const head = self.head.load(.acquire);
            if (head == tail) return null;
            // safe because: .acquire pairs with the producer's ready .release
            // store. A claimed-but-unwritten head-of-line slot reads 0 → treat
            // as empty for this pass; the next flush pass retries.
            if (self.ready[tail].load(.acquire) != 1) return null;
            const entry = self.buffer[tail];
            // safe because: ready clears before the tail .release store, and a
            // producer can only claim this slot after observing the advanced
            // tail — so a fresh claimant always starts from ready == 0.
            self.ready[tail].store(0, .release);
            self.tail.store((tail + 1) % capacity, .release);
            return entry;
        }

        pub fn len(self: *Self) usize {
            // safe because: monotonic-quality snapshot for batching/drain
            // heuristics only; .acquire keeps it no staler than the callers'
            // own loads.
            const head = self.head.load(.acquire);
            const tail = self.tail.load(.acquire);
            if (head >= tail) return head - tail;
            return capacity - tail + head;
        }

        pub fn droppedCount(self: *Self) u64 {
            return self.dropped.load(.acquire);
        }
    };
}
