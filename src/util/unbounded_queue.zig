//! Lock-free MPMC queue for pointer-sized payloads, with batch push/pop.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/threading/unbounded_queue.zig`, MIT, commit
//! `dc578b12eca413e16b6bbea117ff24b73b48187f`). Stripped: the
//! `next_field: meta.FieldEnum(T)` parameter and the custom-accessor branch
//! for packed-pointer types — we hard-code the linkage to a `next: ?T`
//! field on the pointee, the simplest convention for in-tree callers and
//! enough for every currently-anticipated use site. Nodes are caller-owned;
//! the queue stores no allocator. `bun.Environment.allow_assert` and
//! `bun.assertf` collapse to `std.debug.assert`.
//!
//! Acquire/release synchronizes-with semantics are preserved; cache-line
//! padding is preserved (head and tail on separate lines to avoid false
//! sharing). T must be `*SomeStruct` whose pointee declares
//! `next: ?T = null`.

const std = @import("std");
const builtin = @import("builtin");
const atomic = std.atomic;

pub const cache_line_length = switch (builtin.target.cpu.arch) {
    .x86_64, .aarch64, .powerpc64 => 128,
    .arm, .mips, .mips64, .riscv64 => 32,
    .s390x => 256,
    else => 64,
};

pub fn UnboundedQueue(comptime T: type) type {
    comptime {
        const info = @typeInfo(T);
        if (info != .pointer or info.pointer.size != .one) {
            @compileError("UnboundedQueue requires T to be a single-item pointer (e.g. *Node)");
        }
        const Pointee = info.pointer.child;
        if (!@hasField(Pointee, "next")) {
            @compileError("UnboundedQueue payload type must have a `next: ?T` field");
        }
    }

    return struct {
        const Self = @This();
        const queue_padding_length = cache_line_length / 2;

        back: std.atomic.Value(?T) align(queue_padding_length) = .init(null),
        front: std.atomic.Value(?T) align(queue_padding_length) = .init(null),

        pub const Batch = struct {
            front: ?T = null,
            last: ?T = null,
            count: usize = 0,

            pub const Iterator = struct {
                batch: Batch,

                pub fn next(self: *Iterator) ?T {
                    if (self.batch.count == 0) return null;
                    const item = self.batch.front orelse unreachable;
                    self.batch.front = item.next;
                    self.batch.count -= 1;
                    return item;
                }
            };

            pub fn iterator(self: Batch) Iterator {
                return .{ .batch = self };
            }
        };

        pub fn init() Self {
            return .{};
        }

        pub fn deinit(self: *Self) void {
            // Caller owns nodes; nothing to free here. Reset state defensively
            // so a re-`init`-ed queue starts clean.
            self.back.store(null, .monotonic);
            self.front.store(null, .monotonic);
        }

        pub fn push(self: *Self, item: T) void {
            self.pushBatch(item, item, 1);
        }

        pub fn pushBatch(self: *Self, first: T, last: T, count: usize) void {
            _ = count; // count is informational; the linked list itself carries length
            atomicStoreNext(last, null, .release);
            if (self.back.swap(last, .acq_rel)) |old_back| {
                atomicStoreNext(old_back, first, .release);
            } else {
                self.front.store(first, .release);
            }
        }

        pub fn pop(self: *Self) ?T {
            var first = self.front.load(.acquire) orelse return null;
            const next_item = while (true) {
                const next_ptr = atomicLoadNext(first, .acquire);
                const maybe_first = self.front.cmpxchgWeak(first, next_ptr, .release, .acquire) orelse
                    break next_ptr;
                first = maybe_first orelse return null;
            };
            if (next_item != null) return first;

            if (self.back.cmpxchgStrong(first, null, .monotonic, .monotonic)) |back| {
                std.debug.assert(back != null);
            } else {
                return first;
            }

            const new_first = while (true) : (atomic.spinLoopHint()) {
                break atomicLoadNext(first, .acquire) orelse continue;
            };

            self.front.store(new_first, .release);
            return first;
        }

        pub fn popBatch(self: *Self) Batch {
            var batch: Batch = .{};

            const first = self.front.swap(null, .acquire) orelse return batch;
            batch.count += 1;

            const last = self.back.swap(null, .monotonic).?;
            var cursor = first;
            while (cursor != last) : (batch.count += 1) {
                cursor = while (true) : (atomic.spinLoopHint()) {
                    break atomicLoadNext(cursor, .acquire) orelse continue;
                };
            }

            batch.front = first;
            batch.last = last;
            return batch;
        }

        pub fn isEmpty(self: *Self) bool {
            return self.back.load(.acquire) == null;
        }

        inline fn atomicLoadNext(item: T, ordering: std.builtin.AtomicOrder) ?T {
            return @atomicLoad(?T, &item.next, ordering);
        }

        inline fn atomicStoreNext(item: T, ptr: ?T, ordering: std.builtin.AtomicOrder) void {
            @atomicStore(?T, &item.next, ptr, ordering);
        }
    };
}
